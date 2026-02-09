package com.solobandultra.app

import android.media.AudioAttributes
import android.media.AudioManager
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.ui.Modifier
import com.solobandultra.app.audio.AudioSessionManager
import com.solobandultra.app.ui.screens.SheetMusicScreen
import com.solobandultra.app.ui.theme.SoloBandUltraTheme

class MainActivity : ComponentActivity() {

    private lateinit var audioSessionManager: AudioSessionManager

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()

        // Initialize audio session manager for silent mode audio playback
        audioSessionManager = AudioSessionManager(this)
        audioSessionManager.configureAudioSession()

        setContent {
            SoloBandUltraTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = MaterialTheme.colorScheme.background
                ) {
                    SheetMusicScreen(
                        onPlayPause = { isPlaying ->
                            if (isPlaying) {
                                audioSessionManager.requestAudioFocus()
                            } else {
                                audioSessionManager.abandonAudioFocus()
                            }
                        },
                        onStop = {
                            audioSessionManager.abandonAudioFocus()
                        }
                    )
                }
            }
        }
    }

    override fun onDestroy() {
        super.onDestroy()
        audioSessionManager.release()
    }
}
