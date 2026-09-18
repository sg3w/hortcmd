#include "hortcmd_plugin.h"
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#ifndef ABI_VERSION
#define ABI_VERSION 1
#endif
static const HortcmdHostV1 *host;
static uint8_t response[1024 * 1024];
static int32_t initialize(const HortcmdHostV1 *value) {
  host = value;
  const char *ping = "{\"type\":\"ping\"}";
  ptrdiff_t count = host->request((const uint8_t *)ping, strlen(ping), response, sizeof(response));
  if (count < 0 || (size_t)count >= sizeof(response)) return 1;
  response[count] = 0;
  return strstr((const char *)response, "pong") == NULL;
}
static int32_t event(const uint8_t *bytes, size_t len) {
  if (len > 4096) return 1;
  char text[4097]; memcpy(text, bytes, len); text[len] = 0;
  if (strstr(text, "crash")) abort();
  if (strstr(text, "hang")) sleep(30);
  const char *request = "{\"type\":\"context:get\"}";
  return host->request((const uint8_t *)request, strlen(request), response, sizeof(response)) < 0;
}
static void shutdown_plugin(void) {}
static const HortcmdPluginV1 api = {ABI_VERSION, sizeof(HortcmdPluginV1), initialize, event, shutdown_plugin};
const HortcmdPluginV1 *hortcmd_plugin_v1(void) { return &api; }
