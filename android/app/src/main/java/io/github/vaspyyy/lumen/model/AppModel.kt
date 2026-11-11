package io.github.vaspyyy.lumen.model

import android.app.Application
import android.content.ClipData
import android.content.ClipboardManager
import android.net.ConnectivityManager
import android.net.Network
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import io.github.vaspyyy.lumen.BuildConfig
import io.github.vaspyyy.lumen.L
import io.github.vaspyyy.lumen.R
import io.github.vaspyyy.lumen.core.Core
import io.github.vaspyyy.lumen.core.TabEvents
import io.github.vaspyyy.lumen.data.Device
import io.github.vaspyyy.lumen.data.Pc
import io.github.vaspyyy.lumen.data.Store
import io.github.vaspyyy.lumen.data.strings
import io.github.vaspyyy.lumen.keys.Authenticator
import io.github.vaspyyy.lumen.keys.KeySigner
import io.github.vaspyyy.lumen.keys.KeyVault
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.util.UUID
import java.util.concurrent.atomic.AtomicLong

enum class Route { Welcome, Scan, TypeCode, Pairing, Terminal }

/** What a pairing code says, read by `Core.offer` without using it. */
data class Offer(val label: String, val user: String, val port: Int, val addresses: List<String>, val hostKeys: List<String> = emptyList()) {
    companion object {
        fun parse(json: String): Result<Offer> = runCatching {
            val o = JSONObject(json)
            if (!o.optBoolean("ok")) error(o.optString("error").ifBlank { "This is not a lumen pairing code." }.replaceFirstChar { it.uppercase() })
            Offer(o.optString("label"), o.optString("user"), o.optInt("port", 22), o.optJSONArray("addresses").strings(), o.optJSONArray("host_keys").strings())
        }
    }
}

/** A pairing link that came from outside the app, waiting for the user to say yes. */
data class PendingLink(val link: String, val offer: Offer)

enum class PairStep { Key, Reach, Confirm, Done, Failed, NoScreenLock }

data class Pairing(
    val offer: Offer,
    val link: String,
    val step: PairStep,
    val error: String = "",
    /** On failure: the step that failed. */
    val failedAt: PairStep = PairStep.Key,
    /** The fingerprint of this phone's key ("SHA256:..."), which the PC shows too when it asks. */
    val fingerprint: String = "",
)

/** The connection to the current PC. */
sealed interface Conn {
    data object Idle : Conn
    data object Demo : Conn
    data object Checking : Conn
    /** Waiting for the owner at the system prompt. */
    data object Auth : Conn
    data class Connecting(val addresses: List<String>) : Conn
    data class Connected(val link: Long, val keeps: Boolean, val address: String = "") : Conn
    /** Lost, and trying again in [seconds]. */
    data class Reconnecting(val attempt: Int, val seconds: Int, val reason: String) : Conn
    /** [network]: the PC did not answer, which a network that is back can change. */
    data class Failed(val message: String, val network: Boolean = false) : Conn
    data object AuthCancelled : Conn
    /** The PC no longer accepts this phone (it was unpaired there). */
    data object Refused : Conn
    /** Something else answered at the PC's address. */
    data object WrongHost : Conn
    /** The key is gone for good: the PC has to be paired again. */
    data object KeyGone : Conn
    data object NoScreenLock : Conn
}

/** A terminal the PC keeps that is not open here. */
data class Kept(val name: String, val title: String, val cwd: String)

data class Grid(val cols: Int, val rows: Int, val cellWidth: Int, val cellHeight: Int)

data class Notice(val id: Int, val text: String)

/**
 * The numbers the core knows pairings by. The core lives as long as the
 * process, longer than one model: so do the numbers.
 */
private val enrollIds = AtomicLong()

/** The most text Android's clipboard is given at once: it takes about a megabyte, at two bytes a character. */
private const val CLIPBOARD_MAX = 400_000

class AppModel(app: Application) : AndroidViewModel(app) {
    private val store = Store(app)
    private val main = Handler(Looper.getMainLooper())
    private val io = Dispatchers.IO

    var coreError by mutableStateOf<String?>(null)
        private set
    var coreVersion by mutableStateOf("")
        private set

    var route by mutableStateOf(Route.Welcome)
        private set
    /** Where Back from the scan or code screens goes. */
    private var pairFrom = Route.Welcome

    var pcs by mutableStateOf(store.load())
        private set
    var currentId by mutableStateOf(store.currentPc)
        private set
    val current: Pc? get() = pcs.firstOrNull { it.id == currentId } ?: pcs.firstOrNull()

    var conn by mutableStateOf<Conn>(Conn.Idle)
        private set
    val tabs = mutableStateListOf<Tab>()
    var active by mutableIntStateOf(0)
    var kept by mutableStateOf<List<Kept>>(emptyList())
        private set

    var pending by mutableStateOf<PendingLink?>(null)
        private set
    var pairing by mutableStateOf<Pairing?>(null)
        private set
    var notice by mutableStateOf<Notice?>(null)
        private set
    private var noticeId = 0

    var fontSize by mutableFloatStateOf(store.fontSize)
        private set
    var grid by mutableStateOf<Grid?>(null)
        private set

    /**
     * For what has to be finished whoever stops caring: giving a connection or
     * a tab back to the core, deleting a key. Not tied to this model's life.
     */
    private val detached = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    private var connectJob: Job? = null
    private var connGen = 0
    /** The owner was wanted while the app was away: logging in goes on when it is in front again. */
    private var waitsForFront = false
    private var pairJob: Job? = null
    private var pairGen = 0
    /** The number the core knows the pairing under way by, or 0. */
    private var enrollId = 0L
    /** Names of terminals that are being opened: a second tap must not open them twice. */
    private val opening = mutableSetOf<String>()
    private var addingTab = false
    /** The key of the pairing under way, until a PC is stored with it: a pairing that ends otherwise takes it along. */
    private var pairKey: String? = null
    private val slug = Device.slug(app)

    /** Keys no paired PC uses go, once, before a pairing makes a new one. */
    private val sweep: Job

    private val network = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) {
            main.post { onNetwork() }
        }
    }

    init {
        try {
            coreVersion = Core.version()
            L.i("core ${coreVersion} loaded")
        } catch (t: Throwable) {
            coreError = t.message ?: t.javaClass.simpleName
            L.e("core: could not load liblumen_mobile", t)
        }
        if (coreError == null && current != null) route = Route.Terminal
        val used = pcs.map { it.keyAlias }.toSet()
        // only when every pairing could be read: otherwise a key might still be wanted
        sweep = detached.launch { if (store.intact) KeyVault.sweep(used) }
        runCatching { app.getSystemService(ConnectivityManager::class.java).registerDefaultNetworkCallback(network) }
    }

    private var started = false

    /**
     * Called when the activity is there. A pairing link that opened the app
     * is asked about before anything else: logging in to the PC that is
     * stored would put the system's prompt in front of that question.
     */
    fun start(link: String?) {
        if (link != null) receiveLink(link)
        if (started) return
        started = true
        if (link == null && coreError == null && current != null) connect()
    }

    /** The app is in front again (and when it starts). */
    fun onFront() {
        if (!started) return
        when (val c = conn) {
            is Conn.Connected -> checkAlive()
            Conn.AuthCancelled -> if (waitsForFront) connect()
            // it may have been the network, which may be back
            is Conn.Failed -> if (c.network) connect()
            else -> Unit
        }
    }

    /** The phone has a network (again, or another one). */
    private fun onNetwork() {
        if (!started) return
        when (val c = conn) {
            // what was connected over the old network is gone with it: find out now, not at the next keepalive
            is Conn.Connected -> checkAlive()
            is Conn.Failed -> if (c.network) connect(auto = true)
            is Conn.Reconnecting -> connect(auto = true)
            else -> Unit
        }
    }

    override fun onCleared() {
        runCatching { getApplication<Application>().getSystemService(ConnectivityManager::class.java).unregisterNetworkCallback(network) }
        // a pairing under way ends with the model: nobody is left to store what it would bring
        stopEnroll()
        dropTabs(kill = false)
        (conn as? Conn.Connected)?.let { c -> detached.launch { runCatching { Core.disconnect(c.link) } } }
    }

    /**
     * A blocking call of the core, or of the key store, that makes something
     * which has to be given back: a connection, a tab, a key. If whoever
     * asked is gone before the answer is there, [unused] gets it instead.
     */
    private suspend fun <T> made(unused: (T) -> Unit, call: () -> T): T {
        val answer = detached.async { call() }
        try {
            return answer.await()
        } catch (e: CancellationException) {
            detached.launch { runCatching { unused(answer.await()) } }
            throw e
        }
    }

    private fun str(id: Int, vararg args: Any) = getApplication<Application>().getString(id, *args)

    fun notify(text: String) {
        notice = Notice(++noticeId, text)
    }

    fun clearNotice(id: Int) {
        if (notice?.id == id) notice = null
    }

    /** Puts text on the clipboard. False if Android did not take it (it takes about 500 000 characters at most). */
    fun copy(text: String, notice: Boolean = true): Boolean {
        val cm = getApplication<Application>().getSystemService(ClipboardManager::class.java)
        val took = text.length <= CLIPBOARD_MAX && runCatching { cm.setPrimaryClip(ClipData.newPlainText("lumen", text)) }.isSuccess
        if (!took) {
            L.w("copy: ${text.length} characters are more than the clipboard takes")
            notify(str(R.string.copy_too_long))
            return false
        }
        // Android 13 and later show their own preview of what was copied.
        if (notice && android.os.Build.VERSION.SDK_INT < 33) notify(str(R.string.copied))
        return true
    }

    fun clipboardText(): String? {
        val cm = getApplication<Application>().getSystemService(ClipboardManager::class.java)
        val clip = cm.primaryClip ?: return null
        if (clip.itemCount == 0) return null
        return clip.getItemAt(0).coerceToText(getApplication()).toString()
    }

    // ---------------------------------------------------------------- navigation

    fun goScan() {
        if (route != Route.TypeCode) pairFrom = route
        route = Route.Scan
    }

    fun goTypeCode() {
        if (route != Route.Scan) pairFrom = route
        route = Route.TypeCode
    }

    /** Back from the scan, code or pairing screens. Returns false if there is nowhere to go back to. */
    fun back(): Boolean = when (route) {
        Route.Scan, Route.TypeCode -> { route = pairFrom; true }
        Route.Pairing -> { cancelPairing(); true }
        Route.Terminal -> if (conn == Conn.Demo) { leaveDemo(); true } else false
        Route.Welcome -> false
    }

    fun pairAnother() {
        pairFrom = route
        route = Route.Scan
    }

    // ---------------------------------------------------------------- pairing

    suspend fun offer(link: String): Result<Offer> = withContext(io) {
        runCatching { Core.offer(link.trim()) }.fold({ Offer.parse(it) }, { Result.failure(it) })
    }

    /** A `lumen://pair/...` link that arrived from outside: never used without asking. */
    fun receiveLink(link: String) {
        if (coreError != null) return
        L.i("link: a pairing link arrived from outside")
        viewModelScope.launch {
            val read = offer(link)
            // The question on screen stays the one that was asked: a link that
            // comes while it stands can't take its place under the finger.
            if (pending != null) return@launch
            read.onSuccess { pending = PendingLink(link.trim(), it) }
                .onFailure {
                    notify(it.message ?: str(R.string.not_a_code))
                    connectStored()
                }
        }
    }

    fun dismissPending() {
        pending = null
        connectStored()
    }

    /** A link came first when it opened the app: once it is dealt with, the PC that is stored gets its turn. */
    private fun connectStored() {
        if (route == Route.Terminal && conn == Conn.Idle && current != null) connect()
    }

    fun acceptPending() {
        val p = pending ?: return
        pending = null
        pairFrom = if (route == Route.Pairing) Route.Welcome else route
        startPairing(p.link, p.offer)
    }

    /** The host key a PC proved to have when it was paired, or "". */
    private fun proven(pc: Pc): String = runCatching { JSONObject(pc.host).optString("proven") }.getOrDefault("")

    /**
     * The pairings this phone has with the machine that proved the host key
     * [machine], for the same user there. What a pairing code says about a
     * PC's keys is not enough to be that PC: it has to prove one.
     */
    private fun pairedBefore(user: String, machine: String): List<Pc> =
        if (machine.isBlank()) emptyList() else pcs.filter { it.user == user && proven(it) == machine }

    fun startPairing(link: String, offer: Offer) {
        stopEnroll()
        pairJob?.cancel()
        val gen = ++pairGen
        val id = enrollIds.incrementAndGet()
        enrollId = id
        val ctx = getApplication<Application>()
        if (route != Route.Pairing && route != Route.Scan && route != Route.TypeCode) pairFrom = route
        route = Route.Pairing
        pairing = Pairing(offer, link, PairStep.Key)
        L.i("pair: with ${offer.label} as ${offer.user}, ${offer.addresses.size} addresses, port ${offer.port}")
        pairJob = viewModelScope.launch {
            if (!KeyVault.hasScreenLock(ctx)) {
                L.i("pair: the phone has no screen lock")
                pairing = pairing?.copy(step = PairStep.NoScreenLock)
                return@launch
            }
            sweep.join()
            val alias = KeyVault.newAlias()
            L.i("pair: making a key in the secure hardware")
            pairKey = alias
            // a pairing that is given up while the key is made must not leave the key behind
            val made = made(unused = { KeyVault.delete(alias, quiet = true) }) { KeyVault.create(ctx, alias) }
            val pub = when (made) {
                is KeyVault.Made.Ok -> {
                    L.i("pair: key made (StrongBox: ${made.strongBox})")
                    withContext(io) { runCatching { Core.publicKey(made.publicKey.encoded) }.getOrDefault("") }
                }
                KeyVault.Made.NoScreenLock -> {
                    pairing = pairing?.copy(step = PairStep.NoScreenLock)
                    return@launch
                }
                is KeyVault.Made.Failed -> {
                    pairing = pairing?.copy(step = PairStep.Failed, error = str(R.string.pair_key_failed, made.message))
                    return@launch
                }
            }
            if (pub.isEmpty()) {
                dropPairKey()
                pairing = pairing?.copy(step = PairStep.Failed, failedAt = PairStep.Key, error = str(R.string.pair_key_unusable))
                return@launch
            }
            val print = withContext(io) { runCatching { Core.fingerprint(pub) }.getOrDefault("") }
            pairing = pairing?.copy(step = PairStep.Reach, fingerprint = print)
            L.i("pair: introducing the key to the PC")
            // enroll blocks for the whole exchange; the core says when the PC
            // has the key and is asking its owner.
            val ask = launch {
                while (pairing?.step == PairStep.Reach) {
                    delay(120)
                    if (runCatching { Core.enrollStage(id) }.getOrDefault(0) == 2 && pairing?.step == PairStep.Reach) {
                        pairing = pairing?.copy(step = PairStep.Confirm)
                    }
                }
            }
            // A PC that was paired before is told which key the new one replaces,
            // so that no line of a key that is gone stays behind there. The core
            // names only the keys of the machine that answers and proves to be it.
            val had = pcs.filter { it.user == offer.user }.mapNotNull { old ->
                val key = runCatching { JSONObject(old.host).optString("public_key") }.getOrDefault("")
                val machine = proven(old)
                if (key.isBlank() || machine.isBlank()) null else "$machine $key"
            }
            val res = withContext(io) {
                runCatching { JSONObject(Core.enroll(link, Device.name(ctx), pub, had.joinToString("\n"), id)) }
                    .getOrElse { JSONObject().put("ok", false).put("code", "failed").put("error", it.message ?: "") }
            }
            ask.cancel()
            if (gen != pairGen) return@launch
            if (!res.optBoolean("ok")) {
                val code = res.optString("code")
                L.i("pair: failed ($code)")
                dropPairKey()
                val at = pairing?.step?.takeIf { it == PairStep.Confirm } ?: PairStep.Reach
                pairing = pairing?.copy(step = PairStep.Failed, failedAt = at, error = res.optString("error").ifBlank { str(R.string.pair_failed_generic) })
                return@launch
            }
            val host = res.optJSONObject("host")
            if (host == null) {
                dropPairKey()
                pairing = pairing?.copy(step = PairStep.Failed, failedAt = PairStep.Confirm, error = str(R.string.pair_failed_generic))
                return@launch
            }
            L.i("pair: done")
            // The same PC paired again (it proved the host key the old pairing
            // was made with, for the same user) takes the place of its old
            // entry: the old key goes, the terminals it had open are found again.
            val before = pairedBefore(offer.user, host.optString("proven"))
            if (before.isNotEmpty()) {
                L.i("pair: replaces ${before.size} earlier pairing(s) with this PC")
                if (before.any { it.id == current?.id }) disconnectNow()
                before.forEach { KeyVault.delete(it.keyAlias) }
            }
            val pc = Pc(
                id = UUID.randomUUID().toString(),
                label = offer.label.ifBlank { host.optString("label") },
                user = offer.user,
                host = host.toString(),
                keyAlias = alias,
                addresses = offer.addresses,
                tabs = before.flatMap { it.tabs }.distinct(),
            )
            savePcs(pcs - before.toSet() + pc)
            pairKey = null
            enrollId = 0L
            pairing = pairing?.copy(step = PairStep.Done)
            delay(1700)
            if (gen != pairGen) return@launch
            switchTo(pc, openFirst = true)
            pairing = null
        }
    }

    fun retryPairing() {
        val p = pairing ?: return
        startPairing(p.link, p.offer)
    }

    /** Ends the exchange with the PC that is under way, if one is: the PC stops asking, and the key made for it goes. */
    private fun stopEnroll() {
        if (enrollId != 0L) {
            if (pairJob?.isActive == true) L.i("pair: stopped")
            // by its number: the exchange may not have begun yet, and it still must not
            runCatching { Core.cancelEnroll(enrollId) }
            enrollId = 0L
        }
        dropPairKey()
    }

    private fun dropPairKey() {
        pairKey?.let { alias -> detached.launch { KeyVault.delete(alias) } }
        pairKey = null
    }

    fun cancelPairing() {
        pairGen++
        stopEnroll()
        pairJob?.cancel()
        pairing = null
        route = if (pairFrom == Route.Pairing) Route.Welcome else pairFrom
        if (route == Route.Terminal && current == null) route = Route.Welcome
        connectStored()
    }

    // ---------------------------------------------------------------- PCs

    private fun savePcs(list: List<Pc>) {
        pcs = list
        runCatching { store.save(list) }.onFailure { L.e("store: could not save", it) }
    }

    private fun updatePc(id: String, f: (Pc) -> Pc) {
        savePcs(pcs.map { if (it.id == id) f(it) else it })
    }

    fun switchTo(pc: Pc, openFirst: Boolean = false) {
        L.i("pc: switching to ${pc.label}")
        disconnectNow()
        currentId = pc.id
        store.currentPc = pc.id
        route = Route.Terminal
        connect(openFirst)
    }

    fun removePc(pc: Pc) {
        L.i("pc: removing ${pc.label}")
        if (pc.id == current?.id) disconnectNow()
        KeyVault.delete(pc.keyAlias)
        savePcs(pcs.filterNot { it.id == pc.id })
        if (currentId == pc.id) {
            currentId = null
            store.currentPc = null
        }
        val next = current
        if (next == null) {
            conn = Conn.Idle
            route = Route.Welcome
        } else if (route == Route.Terminal && conn == Conn.Idle) {
            switchTo(next)
        }
        notify(str(R.string.removed_pc, pc.label))
    }

    // ---------------------------------------------------------------- connection

    private val link: Long get() = (conn as? Conn.Connected)?.link ?: 0L
    private val keeps: Boolean get() = (conn as? Conn.Connected)?.keeps == true

    /** Logs in to the current PC. With [auto], an attempt that fails for the network's sake is repeated a few times. */
    fun connect(openFirst: Boolean = false, auto: Boolean = false) {
        val pc = current ?: return
        connectJob?.cancel()
        waitsForFront = false
        // what an earlier try missed is not this one's
        Authenticator.takeMissed()
        val gen = ++connGen
        connectJob = viewModelScope.launch { run(pc, gen, auto, openFirst) }
    }

    private enum class Outcome { Ok, Stop, Retry }

    /** How one try to log in ended. [network]: the PC did not answer at all. */
    private class Attempt(val outcome: Outcome, val error: String = "", val network: Boolean = false)

    private val stop = Attempt(Outcome.Stop)

    private suspend fun run(pc: Pc, gen: Int, auto: Boolean, openFirst: Boolean, reason: String = "") {
        val waits = intArrayOf(1, 2, 5)
        var attempt = 0
        var lastError = reason
        // a connection that was lost is the network's doing until a try says otherwise
        var network = true
        while (true) {
            if (auto) {
                if (attempt >= waits.size) {
                    conn = Conn.Failed(lastError.ifBlank { str(R.string.conn_lost_generic) }, network)
                    return
                }
                for (s in waits[attempt] downTo 1) {
                    conn = Conn.Reconnecting(attempt + 1, s, lastError)
                    delay(1000)
                    if (gen != connGen) return
                }
            }
            val tried = attemptOnce(pc, gen, openFirst)
            if (gen != connGen) return
            when (tried.outcome) {
                Outcome.Ok, Outcome.Stop -> return
                Outcome.Retry -> {
                    lastError = tried.error
                    network = tried.network
                    if (!auto) {
                        conn = Conn.Failed(tried.error, network)
                        return
                    }
                    attempt++
                }
            }
        }
    }

    private suspend fun attemptOnce(pc: Pc, gen: Int, openFirst: Boolean): Attempt {
        val ctx = getApplication<Application>()
        conn = Conn.Checking
        if (!KeyVault.hasScreenLock(ctx)) {
            L.i("connect: the phone has no screen lock")
            conn = if (KeyVault.exists(pc.keyAlias)) Conn.NoScreenLock else Conn.KeyGone
            return stop
        }
        L.i("connect: checking the key for ${pc.label}")
        var probe = withContext(io) { KeyVault.probe(pc.keyAlias) }
        if (gen != connGen) return stop
        if (probe == KeyVault.Signed.NeedsAuth) {
            L.i("connect: the key needs the owner")
            conn = Conn.Auth
            val ok = Authenticator.authenticate(str(R.string.auth_title, pc.label), str(R.string.auth_subtitle))
            if (gen != connGen) return stop
            if (!ok) {
                awaitOwner()
                return stop
            }
            probe = withContext(io) { KeyVault.probe(pc.keyAlias) }
        }
        when (probe) {
            is KeyVault.Signed.Ok -> Unit
            KeyVault.Signed.Gone -> {
                conn = Conn.KeyGone
                return stop
            }
            KeyVault.Signed.NeedsAuth -> {
                conn = Conn.AuthCancelled
                return stop
            }
            is KeyVault.Signed.Failed -> {
                conn = Conn.Failed(str(R.string.conn_key_failed, probe.message))
                return stop
            }
        }
        val addresses = hostAddresses(pc)
        conn = Conn.Connecting(addresses)
        L.i("connect: logging in to ${pc.label} (${addresses.joinToString()})")
        val signer = KeySigner(pc.keyAlias, str(R.string.auth_title, pc.label), str(R.string.auth_subtitle))
        // a connection nobody waits for any more is closed again, not left open
        val giveBack = { r: JSONObject -> if (r.optBoolean("ok")) Core.disconnect(r.optLong("link")) }
        val res = made(giveBack) {
            runCatching { JSONObject(Core.connect(pc.host, signer)) }
                .getOrElse { JSONObject().put("ok", false).put("code", "failed").put("error", it.message ?: "") }
        }
        if (gen != connGen) {
            detached.launch { runCatching { giveBack(res) } }
            return stop
        }
        if (res.optBoolean("ok")) {
            val l = res.optLong("link")
            val k = res.optBoolean("keeps")
            L.i("connect: connected (link $l, keeps terminals: $k)")
            conn = Conn.Connected(l, k, res.optString("address"))
            onConnected(pc, openFirst)
            return Attempt(Outcome.Ok)
        }
        val code = res.optString("code")
        val error = res.optString("error").ifBlank { str(R.string.conn_failed_generic) }
        L.i("connect: failed ($code)")
        return when (code) {
            "refused" -> { conn = Conn.Refused; stop }
            "wrong-host" -> { conn = Conn.WrongHost; stop }
            "cancelled" -> { awaitOwner(); stop }
            else -> Attempt(Outcome.Retry, error, network = code == "unreachable")
        }
    }

    /** The owner did not unlock the key. If that is because the app was away, logging in goes on when it is back. */
    private fun awaitOwner() {
        conn = Conn.AuthCancelled
        waitsForFront = Authenticator.takeMissed()
    }

    private fun hostAddresses(pc: Pc): List<String> {
        val fromHost = runCatching { JSONObject(pc.host).optJSONArray("addresses").strings() }.getOrDefault(emptyList())
        return fromHost.ifEmpty { pc.addresses }
    }

    private suspend fun onConnected(pc: Pc, openFirst: Boolean) {
        if (!keeps) {
            // Plain shells end with the connection: nothing to reattach.
            tabs.toList().forEach { closeHandle(it) }
            tabs.clear()
            openTab("$slug-1", persist = false)
            return
        }
        if (tabs.isNotEmpty()) {
            L.i("connect: reattaching ${tabs.size} tabs")
            tabs.toList().forEach { reopen(it) }
        } else {
            val names = current?.tabs.orEmpty()
            if (names.isNotEmpty()) L.i("connect: reattaching ${names.size} remembered tabs")
            names.forEach { openTab(it, persist = false) }
        }
        if (tabs.isEmpty() && openFirst) newTab()
        if (tabs.isEmpty()) refreshKept()
    }

    /** Lets go of the connection and the tabs (their terminals keep running on the PC). */
    private fun disconnectNow() {
        connGen++
        connectJob?.cancel()
        dropTabs(kill = false)
        val c = conn
        conn = Conn.Idle
        kept = emptyList()
        if (c is Conn.Connected) detached.launch { runCatching { Core.disconnect(c.link) } }
    }

    private fun dropTabs(kill: Boolean) {
        tabs.toList().forEach { closeHandle(it) }
        tabs.clear()
        active = 0
    }

    private fun onLost(eventLink: Long, reason: String) {
        val c = conn as? Conn.Connected ?: return
        if (c.link != eventLink) return
        L.i("connect: lost the connection")
        val pc = current ?: return
        tabs.toList().forEach { closeHandle(it) }
        if (!c.keeps) tabs.clear()
        detached.launch { runCatching { Core.disconnect(c.link) } }
        connectJob?.cancel()
        waitsForFront = false
        val gen = ++connGen
        connectJob = viewModelScope.launch { run(pc, gen, auto = true, openFirst = false, reason = reason) }
    }

    private fun checkAlive() {
        val c = conn as? Conn.Connected ?: return
        viewModelScope.launch {
            val ok = withContext(io) { runCatching { Core.alive(c.link, 3000) }.getOrDefault(false) }
            if (!ok && conn == c) {
                L.i("connect: the PC stopped answering while the app was away")
                onLost(c.link, str(R.string.conn_stopped_answering))
            }
        }
    }

    fun retry() {
        when (conn) {
            Conn.Demo -> Unit
            else -> connect()
        }
    }

    // ---------------------------------------------------------------- tabs

    private suspend fun awaitGrid(): Grid = grid ?: snapshotFlow { grid }.first { it != null }!!

    private fun events(tab: Tab, gen: Int, link: Long) = object : TabEvents {
        private fun onMain(f: () -> Unit) {
            main.post { if (tab.gen == gen) f() }
        }
        override fun dirty() {
            if (tab.gen != gen) return
            tab.dirty.set(true)
            tab.onDirty?.invoke()
        }
        override fun title(title: String) = onMain { tab.title = title }
        override fun bell() = onMain {
            // a program that rings without end is felt a few times a second, not more
            val now = SystemClock.uptimeMillis()
            if (now - tab.lastBell >= 250) {
                tab.lastBell = now
                tab.bells++
            }
        }
        override fun clipboard(text: String) = onMain {
            // only the terminal in front may put something on the clipboard
            if (tabs.getOrNull(active) === tab && copy(text, notice = false)) notify(str(R.string.program_copied))
        }
        override fun ended(status: Int) = onMain { onEnded(tab, status) }
        override fun lost(reason: String) = onMain { onLost(link, reason) }
    }

    private suspend fun openTab(name: String, persist: Boolean, select: Boolean = true) {
        val l = link
        if (l == 0L) return
        // one tab to a terminal: a second tap while the first is still opening does nothing
        if (tabs.any { it.name == name } || !opening.add(name)) return
        try {
            openNow(l, name, persist, select)
        } finally {
            opening -= name
        }
    }

    private suspend fun openNow(l: Long, name: String, persist: Boolean, select: Boolean) {
        val g = awaitGrid()
        val tab = Tab(name)
        tab.gen = 1
        val ev = events(tab, tab.gen, l)
        val h = made(closeUnused) { runCatching { Core.open(l, name, g.cols, g.rows, g.cellWidth, g.cellHeight, ev) }.getOrDefault(0L) }
        if (h == 0L) {
            L.w("tab: could not open $name")
            return
        }
        if (link != l) {
            detached.launch { runCatching { Core.close(h) } }
            return
        }
        tab.handle = h
        tab.cols = g.cols
        tab.rows = g.rows
        tabs += tab
        if (select) active = tabs.lastIndex
        if (persist) current?.let { pc -> updatePc(pc.id) { it.copy(tabs = (it.tabs + name).distinct()) } }
        L.i("tab: opened $name")
        kept = kept.filterNot { it.name == name }
    }

    /** A tab nobody waits for any more lets go of its terminal. */
    private val closeUnused = { h: Long -> if (h != 0L) Core.close(h) }

    private suspend fun reopen(tab: Tab) {
        val l = link
        if (l == 0L) return
        val g = awaitGrid()
        tab.gen++
        val ev = events(tab, tab.gen, l)
        // Not marked dirty: the last screen stays until the PC sends the new one.
        val opened = tab.gen
        val h = made(closeUnused) { runCatching { Core.open(l, tab.name, g.cols, g.rows, g.cellWidth, g.cellHeight, ev) }.getOrDefault(0L) }
        // the connection changed meanwhile, or the tab was closed or ended: nobody wants this terminal shown
        if (link != l || tab.gen != opened || tab !in tabs) {
            detached.launch { runCatching { closeUnused(h) } }
            return
        }
        if (h == 0L) L.w("tab: could not open ${tab.name} again")
        tab.handle = h
        tab.cols = g.cols
        tab.rows = g.rows
    }

    private fun closeHandle(tab: Tab) {
        val h = tab.handle
        tab.handle = 0L
        tab.gen++
        if (h != 0L) detached.launch { runCatching { Core.close(h) } }
    }

    fun newTab() {
        // one at a time: two taps would be given the same free name
        if (addingTab) return
        addingTab = true
        viewModelScope.launch {
            try {
                if (conn == Conn.Demo) {
                    openDemoTab()
                    return@launch
                }
                val l = link
                if (l == 0L) return@launch
                if (!keeps) {
                    openTab(freeName(emptySet()), persist = false)
                    return@launch
                }
                val onPc = withContext(io) { parseSessions(runCatching { Core.sessions(l) }.getOrDefault("[]")) }.map { it.name }
                openTab(freeName(onPc.toSet()), persist = true)
            } finally {
                addingTab = false
            }
        }
    }

    private fun freeName(onPc: Set<String>): String {
        val taken = onPc + tabs.map { it.name } + opening
        var n = 1
        while ("$slug-$n" in taken) n++
        return "$slug-$n"
    }

    fun openKept(k: Kept) {
        viewModelScope.launch { openTab(k.name, persist = true) }
    }

    fun refreshKept() {
        val l = link
        if (l == 0L || !keeps) {
            kept = emptyList()
            return
        }
        viewModelScope.launch {
            val list = withContext(io) { parseSessions(runCatching { Core.sessions(l) }.getOrDefault("[]")) }
            if (link == l) kept = list.filter { k -> tabs.none { it.name == k.name } }
        }
    }

    private fun parseSessions(json: String): List<Kept> = runCatching {
        val arr = JSONArray(json)
        List(arr.length()) {
            val o = arr.getJSONObject(it)
            Kept(o.optString("name"), o.optString("title"), o.optString("cwd"))
        }.filter { it.name.isNotEmpty() }
    }.getOrDefault(emptyList())

    /** Closes a tab. Unless [keepRunning], its terminal on the PC ends too. */
    fun closeTab(tab: Tab, keepRunning: Boolean) {
        val idx = tabs.indexOf(tab)
        if (idx < 0) return
        removeTab(tab)
        val h = tab.handle
        tab.handle = 0L
        tab.gen++
        val l = link
        val kill = !keepRunning && !tab.demo && l != 0L && keeps
        L.i("tab: closing ${tab.name}${if (kill) " and ending it on the PC" else ""}")
        detached.launch {
            if (kill) runCatching { Core.kill(l, tab.name) }
            if (h != 0L) runCatching { Core.close(h) }
        }
        if (tabs.isEmpty()) refreshKept()
    }

    private fun removeTab(tab: Tab) {
        val idx = tabs.indexOf(tab)
        if (idx < 0) return
        tabs.removeAt(idx)
        if (active >= tabs.size) active = (tabs.size - 1).coerceAtLeast(0) else if (idx < active) active--
        if (!tab.demo) current?.let { pc -> updatePc(pc.id) { it.copy(tabs = it.tabs - tab.name) } }
    }

    private fun onEnded(tab: Tab, status: Int) {
        L.i("tab: ${tab.name} ended ($status)")
        removeTab(tab)
        closeHandle(tab)
        if (tabs.isEmpty()) refreshKept()
    }

    fun select(index: Int) {
        if (index in tabs.indices) active = index
    }

    // ---------------------------------------------------------------- the view

    fun onGrid(g: Grid) {
        val old = grid
        grid = g
        if (old == g) return
        tabs.forEach { t ->
            if (t.handle != 0L && (t.cols != g.cols || t.rows != g.rows || old?.cellWidth != g.cellWidth)) {
                t.cols = g.cols
                t.rows = g.rows
                runCatching { Core.resize(t.handle, g.cols, g.rows, g.cellWidth, g.cellHeight) }
                t.dirty.set(true)
            }
        }
    }

    fun setFont(size: Float, save: Boolean) {
        fontSize = size.coerceIn(7f, 28f)
        if (save) store.fontSize = fontSize
    }

    // ---------------------------------------------------------------- demo

    /**
     * Debug builds only: a paired PC that is not there (a real key, a host the
     * core cannot reach), for trying the key check, the system prompt and the
     * connection states without a PC.
     */
    fun debugFakePc() {
        if (!BuildConfig.DEBUG || coreError != null) return
        val ctx = getApplication<Application>()
        viewModelScope.launch {
            sweep.join()
            val alias = KeyVault.newAlias()
            when (val made = withContext(io) { KeyVault.create(ctx, alias) }) {
                is KeyVault.Made.Ok -> Unit
                KeyVault.Made.NoScreenLock -> { notify(str(R.string.no_lock_title)); return@launch }
                is KeyVault.Made.Failed -> { notify(made.message); return@launch }
            }
            val host = JSONObject().put("label", "nowhere").put("user", "ada").put("port", 22)
                .put("addresses", JSONArray(listOf("192.0.2.1")))
            val pc = Pc(UUID.randomUUID().toString(), "nowhere", "ada", host.toString(), alias, listOf("192.0.2.1"), emptyList())
            savePcs(pcs + pc)
            switchTo(pc)
        }
    }

    fun openDemo() {
        if (coreError != null) return
        disconnectNow()
        conn = Conn.Demo
        route = Route.Terminal
        viewModelScope.launch { openDemoTab() }
    }

    private suspend fun openDemoTab() {
        val g = awaitGrid()
        val tab = Tab("demo-${tabs.size + 1}", demo = true)
        tab.gen = 1
        val h = runCatching { Core.demo(g.cols, g.rows, events(tab, 1, 0L)) }.getOrDefault(0L)
        if (h == 0L) return
        tab.handle = h
        tab.cols = g.cols
        tab.rows = g.rows
        tabs += tab
        active = tabs.lastIndex
    }

    private fun leaveDemo() {
        dropTabs(kill = false)
        conn = Conn.Idle
        val pc = current
        if (pc != null) switchTo(pc) else route = Route.Welcome
    }
}
