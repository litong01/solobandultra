#ifndef CHOIRLIB_H
#define CHOIRLIB_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/** Start choir server; choir_name and password are UTF-8. Returns port (0 = error). */
uint16_t choir_server_start(const char *choir_name, const char *password);

/** Stop choir server. */
void choir_server_stop(void);

/** After choir_server_start, get listen address "IP:port" for display. Free with choir_free_string. */
char *choir_server_listen_address(void);

/** Discover choirs (blocking, timeout_secs). Returns JSON array; free with choir_free_string. */
char *choir_discover(uint32_t timeout_secs);

/** After choir_discover returned null, get last error message. Free with choir_free_string. */
char *choir_discover_last_error(void);

/** Join choir (blocking). Returns 1 on success. */
int32_t choir_client_join(const char *choir_name, const char *password);

/** Join choir by WebSocket URL (blocking, no mDNS). From Android emulator use ws://10.0.2.2:PORT. Returns 1 on success. */
int32_t choir_client_join_with_url(const char *ws_url, const char *choir_name, const char *password);

/** Leave choir. */
void choir_client_leave(void);

/** Returns 1 if client is still connected (background task running), 0 if disconnected. Use to sync UI (show Join when 0). */
int32_t choir_client_connected(void);

/** Leader: connect as client to own server (call after choir_server_start). Returns 1 on success. */
int32_t choir_leader_connect(uint16_t port);

/** Leader: send command. execute_at_ms = when all clients should execute (use choir_execute_at_ms). */
int32_t choir_send_command(const char *command, int64_t execute_at_ms);

/** After choir_send_command returned 0, get last failure reason. Free with choir_free_string. */
char *choir_send_command_last_error(void);

/** Last reason the leader client task exited (why connection dropped). Free with choir_free_string. */
char *choir_client_exit_reason(void);

/** Last reason the server saw a client disconnect (server's view). Free with choir_free_string. */
char *choir_server_last_disconnect_reason(void);

/** Compute execute_at = now + delay_ms. */
int64_t choir_execute_at_ms(int64_t delay_ms);

/** Poll next command. out_command at least 32 bytes; out_execute_at_ms set. Returns 1 if available. */
int32_t choir_poll_command(char *out_command, size_t out_command_len, int64_t *out_execute_at_ms);

/** Free string from choir_discover. */
void choir_free_string(char *ptr);

#ifdef __cplusplus
}
#endif

#endif /* CHOIRLIB_H */
