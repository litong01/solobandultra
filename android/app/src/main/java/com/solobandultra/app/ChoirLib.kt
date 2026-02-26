package com.solobandultra.app

/**
 * JNI bridge to the Rust choirlib library for choir WebSocket server/client and mDNS discovery.
 */
object ChoirLib {

    init {
        System.loadLibrary("choirlib")
    }

    /** Start choir server. Returns port (0 = error). */
    external fun choirServerStart(choirName: String, password: String): Int

    /** Stop choir server. */
    external fun choirServerStop()

    /** Discover choirs (blocking, timeoutSecs). Returns JSON array of {choir_name, ws_url}; null on error. */
    external fun choirDiscover(timeoutSecs: Int): String?

    /** Join choir (blocking). Returns true on success. */
    external fun choirClientJoin(choirName: String, password: String): Boolean

    /** Join choir by WebSocket URL (blocking, no mDNS). From Android emulator use ws://10.0.2.2:PORT. Returns true on success. */
    external fun choirClientJoinWithUrl(wsUrl: String, choirName: String, password: String): Boolean

    /** Leave choir. */
    external fun choirClientLeave()

    /** True if the client connection is still alive (background task running). When false, the UI should show Join not Leave. */
    external fun choirClientConnected(): Boolean

    /** Leader: connect as client to own server (call after choirServerStart). Returns true on success. */
    external fun choirLeaderConnect(port: Int): Boolean

    /** Send command as leader. executeAtMs from choirExecuteAtMs(delayMs). */
    external fun choirSendCommand(command: String, executeAtMs: Long): Boolean

    /** Compute execute_at = now + delayMs. */
    external fun choirExecuteAtMs(delayMs: Long): Long

    /** Poll next command. Returns JSON "{\"command\":\"play\",\"execute_at_ms\":123}" or null. */
    external fun choirPollCommand(): String?
}
