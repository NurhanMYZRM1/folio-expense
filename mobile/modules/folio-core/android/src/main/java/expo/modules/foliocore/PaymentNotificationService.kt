package expo.modules.foliocore

import android.app.Notification
import android.service.notification.NotificationListenerService
import android.service.notification.StatusBarNotification
import org.json.JSONObject
import java.io.File
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/** Optional Android-only capture. Unrelated packages are rejected before their
 * notification contents are read. Raw text never leaves this local call. */
class PaymentNotificationService : NotificationListenerService() {
  override fun onNotificationPosted(sbn: StatusBarNotification) {
    if (sbn.packageName !in setOf("my.com.maybank2u.m2umobile", "my.com.tngdigital.ewallet")) return
    if (sbn.notification.flags and Notification.FLAG_GROUP_SUMMARY != 0) return
    try {
      NativeCore.initialize(applicationContext)
      val root = File(noBackupFilesDir, "expense-app").apply { mkdirs() }
      if (JSONObject(NativeCore.open(root.absolutePath)).has("error")) return
      val settings = JSONObject(NativeCore.invoke("get_settings", "{}"))
      if (!settings.optJSONObject("result")!!.optBoolean("paymentAlertsEnabled", false)) return
      val extras = sbn.notification.extras
      val text = extras.getCharSequence(Notification.EXTRA_BIG_TEXT)?.toString()
        ?: extras.getCharSequence(Notification.EXTRA_TEXT)?.toString() ?: return
      val input = JSONObject().put("packageName", sbn.packageName).put("notificationKey", sbn.key)
        .put("postedAt", sbn.postTime).put("text", text.take(4000))
        .put("occurredOn", SimpleDateFormat("yyyy-MM-dd", Locale.US).format(Date(sbn.postTime)))
      NativeCore.invoke("ingest_payment_notification", JSONObject().put("input", input).toString())
    } catch (_: Exception) {
      // No financial content or credential diagnostics; manual entry stays available.
    }
  }
}
