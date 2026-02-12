#ifndef SCORELIB_H
#define SCORELIB_H

#include <stdint.h>
#include <stddef.h>

/**
 * Parse a MusicXML file at the given path and render it to SVG.
 * `page_width` sets the SVG width in user units. Pass 0.0 for the default (820).
 * `transpose` shifts all pitches by this many semitones (0 = no change).
 * Returns a null-terminated SVG string, or NULL on error.
 * The caller must free the returned string with scorelib_free_string().
 */
char* scorelib_render_file(const char* path, double page_width, int32_t transpose);

/**
 * Parse MusicXML data from a byte buffer and render to SVG.
 * `extension` is an optional format hint ("musicxml", "mxl", "xml"), may be NULL.
 * `page_width` sets the SVG width in user units. Pass 0.0 for the default (820).
 * `transpose` shifts all pitches by this many semitones (0 = no change).
 * Returns a null-terminated SVG string, or NULL on error.
 * The caller must free the returned string with scorelib_free_string().
 */
char* scorelib_render_bytes(const uint8_t* data, size_t len, const char* extension, double page_width, int32_t transpose);

/**
 * Generate a playback map JSON string from MusicXML data.
 * The playback map contains measure positions, system positions, and the
 * unrolled timemap — everything needed for cursor synchronization.
 * `extension` is an optional format hint, may be NULL.
 * `page_width` sets the SVG width in user units. Pass 0.0 for the default (820).
 * `transpose` shifts all pitches by this many semitones (0 = no change).
 * Returns a null-terminated JSON string, or NULL on error.
 * The caller must free the returned string with scorelib_free_string().
 */
char* scorelib_playback_map(const uint8_t* data, size_t len, const char* extension, double page_width, int32_t transpose);

/**
 * Generate MIDI (SMF Type 1) bytes from MusicXML data.
 * `extension` is an optional format hint, may be NULL.
 * `options_json` is a JSON string with MIDI generation options, may be NULL for defaults.
 * `out_len` receives the length of the returned MIDI data.
 * Returns a pointer to the MIDI bytes, or NULL on error.
 * The caller must free the returned buffer with scorelib_free_midi().
 */
uint8_t* scorelib_generate_midi_from_bytes(const uint8_t* data, size_t len,
                                           const char* extension,
                                           const char* options_json,
                                           size_t* out_len);

/**
 * Generate MIDI (SMF Type 1) bytes from a MusicXML file path.
 * `options_json` is a JSON string with MIDI generation options, may be NULL for defaults.
 * `out_len` receives the length of the returned MIDI data.
 * Returns a pointer to the MIDI bytes, or NULL on error.
 * The caller must free the returned buffer with scorelib_free_midi().
 */
uint8_t* scorelib_generate_midi(const char* path, const char* options_json, size_t* out_len);

/**
 * Free a string previously returned by scorelib functions.
 * Safe to call with NULL.
 */
void scorelib_free_string(char* ptr);

/**
 * Free MIDI bytes previously returned by scorelib_generate_midi functions.
 * Safe to call with NULL.
 */
void scorelib_free_midi(uint8_t* ptr, size_t len);

#endif /* SCORELIB_H */
