package com.solobandultra.app

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import com.solobandultra.app.audio.AudioSessionManager
import com.solobandultra.app.audio.PlaybackManager
import com.solobandultra.app.ui.screens.SheetMusicScreen
import com.solobandultra.app.ui.theme.SoloBandUltraTheme

class MainActivity : ComponentActivity() {

    private lateinit var audioSessionManager: AudioSessionManager
    private lateinit var playbackManager: PlaybackManager

    /** URI of a file opened via "Open With" / file association. */
    private val pendingFileUri = mutableStateOf<Uri?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()

        // Initialize audio session manager for silent mode audio playback
        audioSessionManager = AudioSessionManager(this)
        audioSessionManager.configureAudioSession()

        // Initialize playback manager
        playbackManager = PlaybackManager(this, audioSessionManager)

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
                        onFileUriConsumed = { pendingFileUri.value = null }
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
            pendingFileUri.value = intent.data
        }
    }

    override fun onDestroy() {
        super.onDestroy()
        playbackManager.release()
    }
}
