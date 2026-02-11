package com.solobandultra.app

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.ui.Modifier
import com.solobandultra.app.audio.AudioSessionManager
import com.solobandultra.app.audio.PlaybackManager
import com.solobandultra.app.ui.screens.SheetMusicScreen
import com.solobandultra.app.ui.theme.SoloBandUltraTheme

class MainActivity : ComponentActivity() {

    private lateinit var audioSessionManager: AudioSessionManager
    private lateinit var playbackManager: PlaybackManager

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()

        // Initialize audio session manager for silent mode audio playback
        audioSessionManager = AudioSessionManager(this)
        audioSessionManager.configureAudioSession()

        // Initialize playback manager
        playbackManager = PlaybackManager(this, audioSessionManager)

        setContent {
            SoloBandUltraTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = MaterialTheme.colorScheme.background
                ) {
                    SheetMusicScreen(
                        playbackManager = playbackManager
                    )
                }
            }
        }
    }

    override fun onDestroy() {
        super.onDestroy()
        playbackManager.release()
    }
}
