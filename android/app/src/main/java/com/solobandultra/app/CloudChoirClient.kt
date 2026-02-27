package com.solobandultra.app

import android.os.Handler
import android.os.Looper
import android.util.Log
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import org.json.JSONObject
import java.util.concurrent.TimeUnit

/**
 * Cloud choir WebSocket client. Connect to wss://base/ws with Kinde Bearer token;
 * send join(room, password). First joiner creates the room. Protocol per docs/websocket.md.
 */
class CloudChoirClient(
    private val baseUrl: String,
    private val token: String,
    private val room: String,
    private val password: String,
    private val onJoined: () -> Unit,
    private val onLeft: (String) -> Unit,
    private val onCommand: (command: String, executeAtMs: Long) -> Unit
) {
    private var webSocket: WebSocket? = null
    private var joinReceived = false
    private var serverOffsetMs = 0L
    private val mainHandler = Handler(Looper.getMainLooper())

    private val client = OkHttpClient.Builder()
        .connectTimeout(15, TimeUnit.SECONDS)
        .readTimeout(0, TimeUnit.MILLISECONDS)
        .writeTimeout(15, TimeUnit.SECONDS)
        .build()

    private fun wsUrl(): String {
        val trimmed = baseUrl.trim().removeSuffix("/")
        val scheme = if (trimmed.startsWith("https", ignoreCase = true)) "wss" else "ws"
        val host = trimmed
            .removePrefix("https://").removePrefix("http://")
        return "$scheme://$host/ws"
    }

    suspend fun connect() = withContext(Dispatchers.IO) {
        val url = wsUrl()
        val request = Request.Builder()
            .url(url)
            .addHeader("Authorization", "Bearer $token")
            .build()

        val deferred = CompletableDeferred<Result<Unit>>()
        var joinClientUtc = ""

        val listener = object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                joinClientUtc = iso8601Utc(System.currentTimeMillis())
                val joinObj = JSONObject().put(
                    "join",
                    JSONObject()
                        .put("room", room)
                        .put("password", password)
                        .put("clientUtc", joinClientUtc)
                )
                webSocket.send(joinObj.toString())
            }

            override fun onMessage(webSocket: WebSocket, text: String) {
                if (!joinReceived) {
                    joinReceived = true
                    val result = parseJoinResponse(text, joinClientUtc)
                    deferred.complete(result)
                    return
                }
                parseCommand(text)?.let { (cmd, at) ->
                    mainHandler.post { onCommand(cmd, at) }
                }
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                Log.e(TAG, "WebSocket failed", t)
                if (!joinReceived) {
                    joinReceived = true
                    deferred.complete(Result.failure(t))
                } else {
                    mainHandler.post { onLeft(t.message ?: "connection failed") }
                }
            }

            override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
                if (joinReceived) {
                    mainHandler.post { onLeft(reason.ifEmpty { "connection closed" }) }
                }
            }
        }

        this@CloudChoirClient.webSocket = client.newWebSocket(request, listener)
        deferred.await().getOrThrow()
        mainHandler.post { onJoined() }
    }

    private fun parseJoinResponse(text: String, clientUtc: String): Result<Unit> {
        return try {
            val obj = JSONObject(text)
            if (obj.has("error")) return Result.failure(Exception(obj.optString("error", "join failed")))
            if (!obj.optBoolean("ok", false)) return Result.failure(Exception("Invalid join response"))
            val serverUtc = obj.optString("serverUtc", "")
            val clientMs = iso8601ToMs(clientUtc) ?: 0L
            val serverMs = iso8601ToMs(serverUtc) ?: 0L
            serverOffsetMs = serverMs - clientMs
            Result.success(Unit)
        } catch (e: Exception) {
            Result.failure(e)
        }
    }

    private fun parseCommand(text: String): Pair<String, Long>? {
        return try {
            val obj = JSONObject(text)
            val (cmd, startAt) = when {
                obj.has("play") -> "play" to obj.getJSONObject("play").optString("startAt", "")
                obj.has("stop") -> "stop" to obj.getJSONObject("stop").optString("startAt", "")
                obj.has("pause") -> "pause" to obj.getJSONObject("pause").optString("startAt", "")
                obj.has("prev") -> "prev" to obj.getJSONObject("prev").optString("startAt", "")
                obj.has("next") -> "next" to obj.getJSONObject("next").optString("startAt", "")
                else -> return null
            }
            val serverMs = iso8601ToMs(startAt) ?: return null
            val executeAtMs = serverMs - serverOffsetMs
            cmd to executeAtMs
        } catch (_: Exception) {
            null
        }
    }

    fun sendCommand(command: String) {
        val at = System.currentTimeMillis() + 500
        val startAt = iso8601Utc(at)
        val body = JSONObject().put(command, JSONObject().put("startAt", startAt).put("comment", command))
        webSocket?.send(body.toString())
    }

    fun disconnect() {
        webSocket?.close(1000, null)
        webSocket = null
    }

    companion object {
        private const val TAG = "CloudChoir"
        private fun iso8601Utc(ms: Long): String =
            java.text.SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss.SSS'Z'", java.util.Locale.US).apply {
                timeZone = java.util.TimeZone.getTimeZone("UTC")
            }.format(java.util.Date(ms))
        private fun iso8601ToMs(s: String): Long? = try {
            java.text.SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss.SSS'Z'", java.util.Locale.US).apply {
                timeZone = java.util.TimeZone.getTimeZone("UTC")
                isLenient = true
            }.parse(s)?.time
        } catch (_: Exception) {
            try {
                java.text.SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss'Z'", java.util.Locale.US).apply {
                    timeZone = java.util.TimeZone.getTimeZone("UTC")
                    isLenient = true
                }.parse(s)?.time
            } catch (_: Exception) { null }
        }
    }
}
