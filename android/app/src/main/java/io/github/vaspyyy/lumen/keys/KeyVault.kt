package io.github.vaspyyy.lumen.keys

import android.app.KeyguardManager
import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import android.security.keystore.StrongBoxUnavailableException
import android.security.keystore.UserNotAuthenticatedException
import io.github.vaspyyy.lumen.L
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.PrivateKey
import java.security.PublicKey
import java.security.Signature
import java.security.spec.ECGenParameterSpec
import java.util.UUID

/**
 * The phone's keys, one per paired PC, in the Android Keystore: EC P-256,
 * for signing only, in StrongBox when the phone has it. A key signs only
 * for 20 seconds after the phone's owner proved who they are (unlocking the
 * phone counts). The private half can never leave the secure hardware.
 */
object KeyVault {
    private const val STORE = "AndroidKeyStore"
    private const val AUTH_SECONDS = 20
    private val PROBE = "lumen: can this key sign right now?".toByteArray()

    sealed interface Made {
        data class Ok(val alias: String, val publicKey: PublicKey, val strongBox: Boolean) : Made
        /** The phone has no screen lock: such a key cannot be made. */
        data object NoScreenLock : Made
        data class Failed(val message: String) : Made
    }

    sealed interface Signed {
        class Ok(val signature: ByteArray) : Signed
        /** The owner has to prove who they are first (fingerprint, face or the screen lock). */
        data object NeedsAuth : Signed
        /** The key is gone for good (the screen lock was removed, or it was deleted). */
        data object Gone : Signed
        data class Failed(val message: String) : Signed
    }

    fun hasScreenLock(context: Context): Boolean =
        context.getSystemService(KeyguardManager::class.java)?.isDeviceSecure == true

    fun newAlias(): String = "lumen-pc-" + UUID.randomUUID().toString()

    fun create(context: Context, alias: String): Made {
        if (!hasScreenLock(context)) return Made.NoScreenLock
        return try {
            Made.Ok(alias, generate(alias, strongBox = true), strongBox = true)
        } catch (e: StrongBoxUnavailableException) {
            L.i("key: no StrongBox on this phone, using the TEE")
            try {
                Made.Ok(alias, generate(alias, strongBox = false), strongBox = false)
            } catch (e2: Exception) {
                failed(context, alias, e2)
            }
        } catch (e: Exception) {
            failed(context, alias, e)
        }
    }

    private fun failed(context: Context, alias: String, e: Exception): Made {
        delete(alias)
        if (!hasScreenLock(context)) return Made.NoScreenLock
        L.e("key: could not make the key (${e.javaClass.simpleName})", e)
        return Made.Failed(e.message ?: e.javaClass.simpleName)
    }

    private fun generate(alias: String, strongBox: Boolean): PublicKey {
        val spec = KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_SIGN)
            .setAlgorithmParameterSpec(ECGenParameterSpec("secp256r1"))
            .setDigests(KeyProperties.DIGEST_SHA256)
            .setUserAuthenticationRequired(true)
            .setUserAuthenticationParameters(
                AUTH_SECONDS,
                KeyProperties.AUTH_BIOMETRIC_STRONG or KeyProperties.AUTH_DEVICE_CREDENTIAL,
            )
            .setIsStrongBoxBacked(strongBox)
            .build()
        val gen = KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_EC, STORE)
        gen.initialize(spec)
        return gen.generateKeyPair().public
    }

    private fun keyStore(): KeyStore = KeyStore.getInstance(STORE).apply { load(null) }

    fun publicKey(alias: String): PublicKey? =
        runCatching { keyStore().getCertificate(alias)?.publicKey }.getOrNull()

    fun exists(alias: String): Boolean = runCatching { keyStore().containsAlias(alias) }.getOrDefault(false)

    fun delete(alias: String) {
        runCatching { keyStore().deleteEntry(alias) }
            .onSuccess { L.i("key: deleted") }
            .onFailure { L.w("key: could not delete it", it) }
    }

    /** Can the key sign right now, without asking the owner? Signs a fixed throwaway text to find out. */
    fun probe(alias: String): Signed = sign(alias, PROBE)

    fun sign(alias: String, data: ByteArray): Signed {
        val key = try {
            keyStore().getKey(alias, null) as? PrivateKey ?: return Signed.Gone
        } catch (e: Exception) {
            return Signed.Gone
        }
        return try {
            val s = Signature.getInstance("SHA256withECDSA")
            s.initSign(key)
            s.update(data)
            Signed.Ok(s.sign())
        } catch (e: UserNotAuthenticatedException) {
            Signed.NeedsAuth
        } catch (e: KeyPermanentlyInvalidatedException) {
            L.w("key: permanently invalidated")
            Signed.Gone
        } catch (e: Exception) {
            // Some keystores wrap the authentication error in a SignatureException.
            if (e.cause is UserNotAuthenticatedException) return Signed.NeedsAuth
            if (e.cause is KeyPermanentlyInvalidatedException) return Signed.Gone
            L.w("key: signing failed (${e.javaClass.simpleName})", e)
            Signed.Failed(e.message ?: e.javaClass.simpleName)
        }
    }
}
