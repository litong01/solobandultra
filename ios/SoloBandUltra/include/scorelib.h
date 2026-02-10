#ifndef SCORELIB_H
#define SCORELIB_H

#include <stdint.h>
#include <stddef.h>

/**
 * Parse a MusicXML file at the given path and render it to SVG.
 * `page_width` sets the SVG width in user units. Pass 0.0 for the default (820).
 * Returns a null-terminated SVG string, or NULL on error.
 * The caller must free the returned string with scorelib_free_string().
 */
char* scorelib_render_file(const char* path, double page_width);

/**
 * Parse MusicXML data from a byte buffer and render to SVG.
 * `extension` is an optional format hint ("musicxml", "mxl", "xml"), may be NULL.
 * `page_width` sets the SVG width in user units. Pass 0.0 for the default (820).
 * Returns a null-terminated SVG string, or NULL on error.
 * The caller must free the returned string with scorelib_free_string().
 */
char* scorelib_render_bytes(const uint8_t* data, size_t len, const char* extension, double page_width);

/**
 * Free a string previously returned by scorelib functions.
 * Safe to call with NULL.
 */
void scorelib_free_string(char* ptr);

#endif /* SCORELIB_H */
