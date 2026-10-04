// C ABI of the Rust core (../../../../src-tauri/src/ffi.rs).
#pragma once

char *folio_open(const char *root);
char *folio_invoke(const char *command, const char *args);
void folio_string_free(char *ptr);
