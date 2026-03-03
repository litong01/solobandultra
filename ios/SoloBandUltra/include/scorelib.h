#ifndef SCORELIB_H
#define SCORELIB_H

#include <stdint.h>
#include <stddef.h>

/**
 * Parse a MusicXML file at the given path and render it to SVG.
 * `page_width` sets the SVG width in user units. Pass 0.0 for the default (820).
 * `transpose` shifts all pitches by this many semitones (0 = no change).
 * `parts_filter` optional comma-separated 1-based part indices (e.g. "1,3,5"). Pass NULL for all parts.
 * `use_jianpu` 1 = Jianpu (numbered) notation, 0 = staff notation.
 * Returns a null-terminated SVG string, or NULL on error.
 * The caller must free the returned string with scorelib_free_string().
 */
char* scorelib_render_file(const char* path, double page_width, int32_t transpose, const char* parts_filter, int32_t use_jianpu);

/**
 * Parse MusicXML data from a byte buffer and render to SVG.
 * `extension` is an optional format hint ("musicxml", "mxl", "xml"), may be NULL.
 * `page_width` sets the SVG width in user units. Pass 0.0 for the default (820).
 * `transpose` shifts all pitches by this many semitones (0 = no change).
 * `parts_filter` optional comma-separated 1-based part indices (e.g. "1,3,5"). Pass NULL for all parts.
 * `use_jianpu` 1 = Jianpu (numbered) notation, 0 = staff notation.
 * Returns a null-terminated SVG string, or NULL on error.
 * The caller must free the returned string with scorelib_free_string().
 */
char* scorelib_render_bytes(const uint8_t* data, size_t len, const char* extension, double page_width, int32_t transpose, const char* parts_filter, int32_t use_jianpu);

/**
 * Generate a playback map JSON string from MusicXML data.
 * The playback map contains measure positions, system positions, and the
 * unrolled timemap — everything needed for cursor synchronization.
 * `extension` is an optional format hint, may be NULL.
 * `page_width` sets the SVG width in user units. Pass 0.0 for the default (820).
 * `transpose` shifts all pitches by this many semitones (0 = no change).
 * `parts_filter` must match the filter used for SVG rendering (e.g. "1,3"). Pass NULL for all.
 * Returns a null-terminated JSON string, or NULL on error.
 * The caller must free the returned string with scorelib_free_string().
 */
char* scorelib_playback_map(const uint8_t* data, size_t len, const char* extension, double page_width, int32_t transpose, const char* parts_filter);

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
 * Render MusicXML data to WAV audio using a SoundFont.
 * Internally generates MIDI and synthesizes it offline.
 * `extension` is an optional format hint, may be NULL.
 * `options_json` is a JSON string with generation options, may be NULL for defaults.
 * `sf_data` points to the SoundFont (SF2) bytes, `sf_len` is its length.
 * `out_len` receives the length of the returned WAV data.
 * Returns a pointer to the WAV bytes, or NULL on error.
 * The caller must free the returned buffer with scorelib_free_midi().
 */
uint8_t* scorelib_render_audio_from_bytes(const uint8_t* data, size_t len,
                                          const char* extension,
                                          const char* options_json,
                                          const uint8_t* sf_data, size_t sf_len,
                                          size_t* out_len);

/**
 * Generate a note timeline JSON array from MusicXML data.
 * Returns melody notes (voice 1, part 0) with absolute timestamps:
 *   [{ "start_ms": 0.0, "end_ms": 250.0, "midi": 60, "name": "C4" }, ...]
 * `extension` is an optional format hint, may be NULL.
 * `transpose` shifts all pitches by this many semitones (0 = no change).
 * Returns a null-terminated JSON string, or NULL on error.
 * The caller must free the returned string with scorelib_free_string().
 */
char* scorelib_note_timeline(const uint8_t* data, size_t len, const char* extension, int32_t transpose);

/**
 * Add the feedback overlay layer (colored dots) to a score SVG.
 * Used for the performance report — dots below each note (green/yellow/red/gray).
 * `svg` and `overlay_dots_json` must be null-terminated UTF-8.
 * `overlay_dots_json` is a JSON array: [ {"x": number, "y": number, "colors": ["#hex", ...]}, ... ]
 * Returns a new SVG string with the overlay inserted, or NULL on error.
 * The caller must free the result with scorelib_free_string().
 */
char* scorelib_add_feedback_overlay(const char* svg, const char* overlay_dots_json);

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
