package expo.modules.foliocore

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** The key stays in Android Keystore. Only authenticated ciphertext is persisted
 * in noBackupFilesDir, so a device restore cannot restore an unusable credential. */
object NativeCore {
  init { System.loadLibrary("folio") }
  private lateinit var context: Context
  private const val ALIAS = "com.folio.expenses.vision"
  fun initialize(value: Context) { context = value.applicationContext }
  @JvmStatic external fun open(root: String): String
  @JvmStatic external fun invoke(command: String, args: String): String
  private fun file() = java.io.File(context.noBackupFilesDir, "folio-vision.enc")
  private fun key(): SecretKey {
    val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
    (store.getKey(ALIAS, null) as? SecretKey)?.let { return it }
    return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").apply {
      init(KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
        .setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE).build())
    }.generateKey()
  }
  @JvmStatic @Synchronized fun readSecret(): String? {
    if (!file().exists()) return null
    val parts = file().readText().split(":")
    require(parts.size == 2) { "Invalid encrypted credential" }
    val cipher = Cipher.getInstance("AES/GCM/NoPadding")
    cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, Base64.decode(parts[0], Base64.NO_WRAP)))
    return String(cipher.doFinal(Base64.decode(parts[1], Base64.NO_WRAP)), Charsets.UTF_8)
  }
  @JvmStatic @Synchronized fun writeSecret(value: String) {
    val cipher = Cipher.getInstance("AES/GCM/NoPadding")
    cipher.init(Cipher.ENCRYPT_MODE, key())
    val encoded = Base64.encodeToString(cipher.iv, Base64.NO_WRAP) + ":" + Base64.encodeToString(cipher.doFinal(value.toByteArray(Charsets.UTF_8)), Base64.NO_WRAP)
    val tmp = java.io.File(context.noBackupFilesDir, "folio-vision.tmp")
    java.io.FileOutputStream(tmp).use { it.write(encoded.toByteArray()); it.fd.sync() }
    check(tmp.renameTo(file())) { "Credential could not be saved" }
  }
  @JvmStatic @Synchronized fun deleteSecret() {
    check(!file().exists() || file().delete()) { "Credential could not be removed" }
  }
}
