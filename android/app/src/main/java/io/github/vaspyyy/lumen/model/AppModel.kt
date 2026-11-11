package io.github.vaspyyy.lumen.model

import android.app.Application
import android.content.ClipData
import android.content.ClipboardManager
import android.os.Handler
import android.os.Looper
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner
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
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.util.UUID

enum class Route { Welcome, Scan, TypeCode, Pairing, Terminal }

/** What a pairing code says, read by `Core.offer` without using it. */
data class Offer(val label: String, val user: String, val port: Int, val addresses: List<String>) {
    companion object {
        fun parse(json: String): Result<Offer> = runCatching {
            val o = JSONObject(json)
            if (!o.optBoolean("ok")) error(o.optString("error").ifBlank { "This is not a lumen pairing code." }.replaceFirstChar { it.uppercase() })
            Offer(o.optString("label"), o.optString("user"), o.optInt("port", 22), o.optJSONArray("addresses").strings())
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
    data class Failed(val message: String) : Conn
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

    private var connectJob: Job? = null
    private var connGen = 0
    private var pairJob: Job? = null
    private var pairGen = 0
    /** The key of the pairing under way, until a PC is stored with it: a pairing that ends otherwise takes it along. */
    private var pairKey: String? = null
    private val slug = Device.slug(app)

    private val foreground = object : DefaultLifecycleObserver {
        override fun onStart(owner: LifecycleOwner) = checkAlive()
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
        ProcessLifecycleOwner.get().lifecycle.addObserver(foreground)
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

    override fun onCleared() {
        ProcessLifecycleOwner.get().lifecycle.removeObserver(foreground)
        dropTabs(kill = false)
        (conn as? Conn.Connected)?.let { c -> Thread { runCatching { Core.disconnect(c.link) } }.start() }
    }

    private fun str(id: Int, vararg args: Any) = getApplication<Application>().getString(id, *args)

    fun notify(text: String) {
        notice = Notice(++noticeId, text)
    }

    fun clearNotice(id: Int) {
        if (notice?.id == id) notice = null
    }

    fun copy(text: String, notice: Boolean = true) {
        val cm = getApplication<Application>().getSystemService(ClipboardManager::class.java)
        cm.setPrimaryClip(ClipData.newPlainText("lumen", text))
        // Android 13 and later show their own preview of what was copied.
        if (notice && android.os.Build.VERSION.SDK_INT < 33) notify(str(R.string.copied))
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
            offer(link).onSuccess { pending = PendingLink(link.trim(), it) }
                .onFailure { notify(it.message ?: str(R.string.not_a_code)) }
        }
    }

    fun dismissPending() {
        pending = null
        // the link came first when it opened the app: now for the PC that is stored
        if (route == Route.Terminal && conn == Conn.Idle && current != null) connect()
    }

    fun acceptPending() {
        val p = pending ?: return
        pending = null
        pairFrom = if (route == Route.Pairing) Route.Welcome else route
        startPairing(p.link, p.offer)
    }

    fun startPairing(link: String, offer: Offer) {
        stopEnroll()
        pairJob?.cancel()
        val gen = ++pairGen
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
            val alias = KeyVault.newAlias()
            L.i("pair: making a key in the secure hardware")
            pairKey = alias
            val made = withContext(io) { KeyVault.create(ctx, alias) }
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
                    if (runCatching { Core.enrollStage() }.getOrDefault(0) == 2 && pairing?.step == PairStep.Reach) {
                        pairing = pairing?.copy(step = PairStep.Confirm)
                    }
                }
            }
            val res = withContext(io) {
                runCatching { JSONObject(Core.enroll(link, Device.name(ctx), pub)) }
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
            // The same PC paired again (it proves itself with a key this phone
            // knows, for the same user) takes the place of its old entry: the
            // old key goes, the terminals it had open are found again.
            val proofs = host.optJSONArray("host_keys").strings().toSet()
            val before = pcs.filter { old ->
                old.user == offer.user &&
                    runCatching { JSONObject(old.host).optJSONArray("host_keys").strings() }.getOrDefault(emptyList()).any { it in proofs }
            }
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
        if (pairJob?.isActive == true) {
            L.i("pair: stopped")
            runCatching { Core.cancelEnroll() }
        }
        dropPairKey()
    }

    private fun dropPairKey() {
        pairKey?.let { KeyVault.delete(it) }
        pairKey = null
    }

    fun cancelPairing() {
        pairGen++
        stopEnroll()
        pairJob?.cancel()
        pairing = null
        route = if (pairFrom == Route.Pairing) Route.Welcome else pairFrom
        if (route == Route.Terminal && current == null) route = Route.Welcome
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

    fun connect(openFirst: Boolean = false) {
        val pc = current ?: return
        connectJob?.cancel()
        val gen = ++connGen
        connectJob = viewModelScope.launch { run(pc, gen, auto = false, openFirst = openFirst) }
    }

    private enum class Outcome { Ok, Stop, Retry }

    private suspend fun run(pc: Pc, gen: Int, auto: Boolean, openFirst: Boolean, reason: String = "") {
        val waits = intArrayOf(1, 2, 5)
        var attempt = 0
        var lastError = reason
        while (true) {
            if (auto) {
                if (attempt >= waits.size) {
                    conn = Conn.Failed(lastError.ifBlank { str(R.string.conn_lost_generic) })
                    return
                }
                for (s in waits[attempt] downTo 1) {
                    conn = Conn.Reconnecting(attempt + 1, s, lastError)
                    delay(1000)
                    if (gen != connGen) return
                }
            }
            val (outcome, error) = attemptOnce(pc, gen, openFirst)
            if (gen != connGen) return
            when (outcome) {
                Outcome.Ok, Outcome.Stop -> return
                Outcome.Retry -> {
                    lastError = error
                    if (!auto) {
                        conn = Conn.Failed(error)
                        return
                    }
                    attempt++
                }
            }
        }
    }

    private suspend fun attemptOnce(pc: Pc, gen: Int, openFirst: Boolean): Pair<Outcome, String> {
        val ctx = getApplication<Application>()
        conn = Conn.Checking
        if (!KeyVault.hasScreenLock(ctx)) {
            L.i("connect: the phone has no screen lock")
            conn = if (KeyVault.exists(pc.keyAlias)) Conn.NoScreenLock else Conn.KeyGone
            return Outcome.Stop to ""
        }
        L.i("connect: checking the key for ${pc.label}")
        var probe = withContext(io) { KeyVault.probe(pc.keyAlias) }
        if (gen != connGen) return Outcome.Stop to ""
        if (probe == KeyVault.Signed.NeedsAuth) {
            L.i("connect: the key needs the owner")
            conn = Conn.Auth
            val ok = Authenticator.authenticate(str(R.string.auth_title, pc.label), str(R.string.auth_subtitle))
            if (gen != connGen) return Outcome.Stop to ""
            if (!ok) {
                conn = Conn.AuthCancelled
                return Outcome.Stop to ""
            }
            probe = withContext(io) { KeyVault.probe(pc.keyAlias) }
        }
        when (probe) {
            is KeyVault.Signed.Ok -> Unit
            KeyVault.Signed.Gone -> {
                conn = Conn.KeyGone
                return Outcome.Stop to ""
            }
            KeyVault.Signed.NeedsAuth -> {
                conn = Conn.AuthCancelled
                return Outcome.Stop to ""
            }
            is KeyVault.Signed.Failed -> {
                conn = Conn.Failed(str(R.string.conn_key_failed, probe.message))
                return Outcome.Stop to ""
            }
        }
        val addresses = hostAddresses(pc)
        conn = Conn.Connecting(addresses)
        L.i("connect: logging in to ${pc.label} (${addresses.joinToString()})")
        val signer = KeySigner(pc.keyAlias, str(R.string.auth_title, pc.label), str(R.string.auth_subtitle))
        val res = withContext(io) {
            runCatching { JSONObject(Core.connect(pc.host, signer)) }
                .getOrElse { JSONObject().put("ok", false).put("code", "failed").put("error", it.message ?: "") }
        }
        if (gen != connGen) {
            if (res.optBoolean("ok")) {
                val stale = res.optLong("link")
                withContext(io) { runCatching { Core.disconnect(stale) } }
            }
            return Outcome.Stop to ""
        }
        if (res.optBoolean("ok")) {
            val l = res.optLong("link")
            val k = res.optBoolean("keeps")
            L.i("connect: connected (link $l, keeps terminals: $k)")
            conn = Conn.Connected(l, k, res.optString("address"))
            onConnected(pc, openFirst)
            return Outcome.Ok to ""
        }
        val code = res.optString("code")
        val error = res.optString("error").ifBlank { str(R.string.conn_failed_generic) }
        L.i("connect: failed ($code)")
        return when (code) {
            "refused" -> { conn = Conn.Refused; Outcome.Stop to error }
            "wrong-host" -> { conn = Conn.WrongHost; Outcome.Stop to error }
            "cancelled" -> { conn = Conn.AuthCancelled; Outcome.Stop to error }
            else -> Outcome.Retry to error
        }
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
        if (c is Conn.Connected) viewModelScope.launch(io) { runCatching { Core.disconnect(c.link) } }
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
        viewModelScope.launch(io) { runCatching { Core.disconnect(c.link) } }
        connectJob?.cancel()
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
        override fun bell() = onMain { tab.bells++ }
        override fun clipboard(text: String) = onMain {
            copy(text, notice = false)
            notify(str(R.string.program_copied))
        }
        override fun ended(status: Int) = onMain { onEnded(tab, status) }
        override fun lost(reason: String) = onMain { onLost(link, reason) }
    }

    private suspend fun openTab(name: String, persist: Boolean, select: Boolean = true) {
        val l = link
        if (l == 0L) return
        val g = awaitGrid()
        val tab = Tab(name)
        tab.gen = 1
        val ev = events(tab, tab.gen, l)
        val h = withContext(io) { runCatching { Core.open(l, name, g.cols, g.rows, g.cellWidth, g.cellHeight, ev) }.getOrDefault(0L) }
        if (h == 0L) {
            L.w("tab: could not open $name")
            return
        }
        if (link != l) {
            withContext(io) { runCatching { Core.close(h) } }
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

    private suspend fun reopen(tab: Tab) {
        val l = link
        if (l == 0L) return
        val g = awaitGrid()
        tab.gen++
        val ev = events(tab, tab.gen, l)
        // Not marked dirty: the last screen stays until the PC sends the new one.
        val h = withContext(io) { runCatching { Core.open(l, tab.name, g.cols, g.rows, g.cellWidth, g.cellHeight, ev) }.getOrDefault(0L) }
        if (link != l) {
            if (h != 0L) withContext(io) { runCatching { Core.close(h) } }
            return
        }
        tab.handle = h
        tab.cols = g.cols
        tab.rows = g.rows
    }

    private fun closeHandle(tab: Tab) {
        val h = tab.handle
        tab.handle = 0L
        tab.gen++
        if (h != 0L) viewModelScope.launch(io) { runCatching { Core.close(h) } }
    }

    fun newTab() {
        viewModelScope.launch {
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
        }
    }

    private fun freeName(onPc: Set<String>): String {
        val taken = onPc + tabs.map { it.name }
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
        viewModelScope.launch(io) {
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
