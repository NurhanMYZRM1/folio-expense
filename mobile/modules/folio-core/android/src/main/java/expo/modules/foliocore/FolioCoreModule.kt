package expo.modules.foliocore

import android.app.Activity
import android.content.Intent
import android.content.ComponentName
import android.provider.Settings
import android.speech.RecognizerIntent
import android.os.Handler
import android.os.Looper
import expo.modules.kotlin.Promise
import expo.modules.kotlin.functions.Queues
import android.graphics.Bitmap
import android.graphics.Color
import android.graphics.pdf.PdfRenderer
import android.net.Uri
import android.os.ParcelFileDescriptor
import com.google.android.gms.tasks.Tasks
import com.google.mlkit.vision.common.InputImage
import com.google.mlkit.vision.text.TextRecognition
import com.google.mlkit.vision.text.latin.TextRecognizerOptions
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition
import java.io.File
import java.util.UUID
import java.util.concurrent.TimeUnit
import kotlin.math.min
import kotlin.math.roundToInt

class FolioCoreModule : Module() {
  private var voice: Promise? = null
  private val handler = Handler(Looper.getMainLooper())
  private val timeout = Runnable { voice?.reject("SpeechUnavailable", "Dictation timed out. Type your transaction instead.", null); voice = null }
  private fun context() = requireNotNull(appContext.reactContext) { "Folio context is unavailable" }
  override fun definition() = ModuleDefinition {
    Name("FolioCore")
    Function("paymentNotificationAccess") {
      val component = ComponentName(context(), PaymentNotificationService::class.java).flattenToString()
      Settings.Secure.getString(context().contentResolver, "enabled_notification_listeners")?.split(":")?.contains(component) == true
    }
    AsyncFunction("requestPaymentNotificationAccess") {
      context().startActivity(Intent(Settings.ACTION_NOTIFICATION_LISTENER_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }.runOnQueue(Queues.MAIN)

    AsyncFunction("transcribeVoice") { promise: Promise ->
      if (voice != null) { promise.reject("SpeechBusy", "Dictation is already running.", null) }
      else {
        val intent = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH)
          .putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
          .putExtra(RecognizerIntent.EXTRA_PROMPT, "Describe one transaction for review")
        try {
          voice = promise
          requireNotNull(appContext.currentActivity).startActivityForResult(intent, 58021)
          handler.postDelayed(timeout, 60000)
        } catch (error: Exception) {
          voice = null
          promise.reject("SpeechUnavailable", "Speech recognition is unavailable. Type your transaction instead.", error)
        }
      }
    }.runOnQueue(Queues.MAIN)
    Function("cancelTranscription") {
      handler.removeCallbacks(timeout)
      voice?.reject("SpeechCancelled", "Dictation cancelled.", null)
      voice = null
    }
    OnActivityResult { _, result ->
      if (result.requestCode == 58021) {
        handler.removeCallbacks(timeout)
        val text = result.data?.getStringArrayListExtra(RecognizerIntent.EXTRA_RESULTS)?.firstOrNull()
        if (result.resultCode == Activity.RESULT_OK && !text.isNullOrBlank()) voice?.resolve(text)
        else voice?.reject("SpeechCancelled", "No speech was saved. Type your transaction instead.", null)
        voice = null
      }
    }
    OnDestroy {
      handler.removeCallbacks(timeout)
      voice?.reject("SpeechCancelled", "Dictation cancelled.", null)
      voice = null
    }

    Function("dataRoot") {
      File(context().noBackupFilesDir, "expense-app").apply { mkdirs() }.absolutePath
    }
    AsyncFunction("open") { root: String ->
      NativeCore.initialize(context())
      NativeCore.open(root)
    }
    AsyncFunction("invoke") { command: String, args: String -> NativeCore.invoke(command, args) }
    AsyncFunction("recognizeText") { path: String ->
      val recognizer = TextRecognition.getClient(TextRecognizerOptions.DEFAULT_OPTIONS)
      try {
        val result = Tasks.await(recognizer.process(InputImage.fromFilePath(context(), Uri.fromFile(File(path)))), 60, TimeUnit.SECONDS)
        // ML Kit's bundled Latin recognizer has no confidence compatible with
        // Vision/Tesseract. Null preserves the core's uncertainty review flow.
        mapOf("text" to result.text, "confidence" to null)
      } finally { recognizer.close() }
    }
    AsyncFunction("renderPdfPages") { path: String, maxPages: Int, maxSide: Double ->
      require(maxPages in 1..30 && maxSide.isFinite() && maxSide in 100.0..3000.0) { "Invalid PDF render limits" }
      val directory = File(context().cacheDir, "folio-pages-${UUID.randomUUID()}").apply { mkdirs() }
      try {
        ParcelFileDescriptor.open(File(path), ParcelFileDescriptor.MODE_READ_ONLY).use { descriptor ->
          PdfRenderer(descriptor).use { pdf ->
            (0 until min(pdf.pageCount, maxPages)).map { index ->
              pdf.openPage(index).use { page ->
                val scale = min(2.5, maxSide / maxOf(page.width, page.height))
                val bitmap = Bitmap.createBitmap((page.width * scale).roundToInt().coerceAtLeast(1), (page.height * scale).roundToInt().coerceAtLeast(1), Bitmap.Config.ARGB_8888)
                try {
                  bitmap.eraseColor(Color.WHITE)
                  page.render(bitmap, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
                  File(directory, "page-${index + 1}.jpg").also { output ->
                    output.outputStream().use { check(bitmap.compress(Bitmap.CompressFormat.JPEG, 85, it)) }
                  }.absolutePath
                } finally { bitmap.recycle() }
              }
            }
          }
        }
      } catch (error: Exception) { directory.deleteRecursively(); throw error }
    }
  }
}
