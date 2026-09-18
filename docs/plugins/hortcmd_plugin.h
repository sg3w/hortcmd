#ifndef HORTCMD_PLUGIN_H
#define HORTCMD_PLUGIN_H
#include <stddef.h>
#include <stdint.h>
/* All buffers belong to the caller. No allocation crosses this ABI. */
typedef struct HortcmdHostV1 {
  uint32_t abi_version;
  size_t struct_size;
  /* Request is JSON; result is {"value":...} or {"error":"..."}.
     Allocate a 1 MiB output buffer. Negative results are transport failures.
     -2 means the response did not fit: do not retry a mutation. */
  ptrdiff_t (*request)(const uint8_t *, size_t, uint8_t *, size_t);
} HortcmdHostV1;
typedef struct HortcmdPluginV1 {
  uint32_t abi_version;
  size_t struct_size;
  int32_t (*initialize)(const HortcmdHostV1 *);
  int32_t (*event)(const uint8_t *, size_t);
  void (*shutdown)(void);
} HortcmdPluginV1;
/* Export this symbol with C calling convention. Keep the returned table alive
   until shutdown returns. initialize/event return 0 on success. */
#ifdef _WIN32
__declspec(dllexport)
#endif
const HortcmdPluginV1 *hortcmd_plugin_v1(void);
#endif
