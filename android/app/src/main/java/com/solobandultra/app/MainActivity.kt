package com.solobandultra.app

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.appcompat.app.AppCompatActivity
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import au.kinde.sdk.GrantType
import au.kinde.sdk.KindeSDK
import com.solobandultra.app.audio.AudioSessionManager
import com.solobandultra.app.audio.PlaybackManager
import com.solobandultra.app.ui.screens.PendingAuthAction
import com.solobandultra.app.ui.screens.SheetMusicScreen
import com.solobandultra.app.ui.theme.SoloBandUltraTheme

class MainActivity : AppCompatActivity() {

    private lateinit var audioSessionManager: AudioSessionManager
    private lateinit var playbackManager: PlaybackManager
    private lateinit var kindeSDK: KindeSDK

    /** URI of a file opened via "Open With" / file association. */
    private val pendingFileUri = mutableStateOf<Uri?>(null)

    /** Whether the user is currently authenticated via Kinde. */
    val isAuthenticated = mutableStateOf(false)

    /** Action to execute after a successful login. */
    val pendingAuthAction = mutableStateOf<PendingAuthAction?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()

        // Initialize audio session manager for silent mode audio playback
        audioSessionManager = AudioSessionManager(this)
        audioSessionManager.configureAudioSession()

        // Initialize playback manager
        playbackManager = PlaybackManager(this, audioSessionManager)

        // Initialize Kinde authentication SDK
        kindeSDK = KindeSDK(
            this,
            object : KindeSDK.SDKListener {
                override fun onNewToken(token: String) {
                    Handler(Looper.getMainLooper()).post {
                        isAuthenticated.value = true
                    }
                }

                override fun onLogout() {
                    Handler(Looper.getMainLooper()).post {
                        isAuthenticated.value = false
                        pendingAuthAction.value = null
                    }
                }

                override fun onException(exception: Exception) {
                    Handler(Looper.getMainLooper()).post {
                        Log.e("Kinde", "Auth error: ${exception.message}")
                        pendingAuthAction.value = null
                    }
                }
            }
        )

        // Check if the user has an existing session
        isAuthenticated.value = kindeSDK.isAuthenticated()

        // Check if launched with a file intent
        handleIntent(intent)

        setContent {
            SoloBandUltraTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = MaterialTheme.colorScheme.background
                ) {
                    SheetMusicScreen(
                        playbackManager = playbackManager,
                        openFileUri = pendingFileUri.value,
                        onFileUriConsumed = { pendingFileUri.value = null },
                        isAuthenticated = isAuthenticated.value,
                        pendingAuthAction = pendingAuthAction.value,
                        onPendingActionConsumed = { pendingAuthAction.value = null },
                        onLoginRequested = { action ->
                            pendingAuthAction.value = action
                            kindeSDK.login(GrantType.PKCE)
                        },
                        onLogoutRequested = {
                            kindeSDK.logout()
                        }
                    )
                }
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleIntent(intent)
    }

    /** Extract a file URI from a VIEW intent. */
    private fun handleIntent(intent: Intent?) {
        if (intent?.action == Intent.ACTION_VIEW) {
            val uri = intent.data ?: return
            if (isAuthenticated.value) {
                pendingFileUri.value = uri
            } else {
                // Store the URI so it can be loaded after login succeeds.
                // We encode the URI string in the pending action; the screen
                // will process it after authentication.
                pendingFileUri.value = uri
                pendingAuthAction.value = PendingAuthAction.LoadExternalUri
                kindeSDK.login(GrantType.PKCE)
            }
        }
    }

    override fun onDestroy() {
        super.onDestroy()
        playbackManager.release()
    }
}
