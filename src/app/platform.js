export function detectPlatform() {
  const platform = navigator.userAgentData?.platform || navigator.platform || "";
  const value = String(platform).toLowerCase();
  if (value.includes("mac")) return "macos";
  if (value.includes("win")) return "windows";
  if (value.includes("linux")) return "linux";
  return "unknown";
}

export function applyPlatform(platform) {
  document.documentElement.dataset.platform = platform || "unknown";
}
