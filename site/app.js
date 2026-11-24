const releaseApi = "https://api.github.com/repos/namannn04/Veronica/releases?per_page=100";
const appTag = /^v(\d+)\.(\d+)\.(\d+)$/;
const fallbackVersion = "0.1.10";

function assetUrl(release, suffix) {
  return release.assets?.find((asset) => asset.name.endsWith(suffix))?.browser_download_url;
}

function appVersion(release) {
  const match = appTag.exec(release.tag_name ?? "");
  return match && !release.draft && !release.prerelease ? match.slice(1).map(Number) : null;
}

function isNewer(version, than) {
  const index = version.findIndex((part, at) => part !== than[at]);
  return index !== -1 && version[index] > than[index];
}

function newestAppRelease(releases) {
  let newest = null;
  let newestVersion = null;
  for (const release of releases) {
    const version = appVersion(release);
    if (version && (!newestVersion || isNewer(version, newestVersion))) {
      newest = release;
      newestVersion = version;
    }
  }
  return newest;
}

async function refreshRelease() {
  try {
    const response = await fetch(releaseApi, {
      headers: { Accept: "application/vnd.github+json" },
    });
    if (!response.ok) {
      return;
    }
    const release = newestAppRelease(await response.json());
    if (!release) {
      return;
    }
    const version = String(release.tag_name || `v${fallbackVersion}`);
    document.querySelectorAll("[data-version]").forEach((node) => {
      node.textContent = version;
    });
    const deb = assetUrl(release, "_amd64.deb");
    const appImage = assetUrl(release, "_amd64.AppImage");
    if (deb) {
      document.querySelectorAll(".download-deb").forEach((link) => {
        link.href = deb;
      });
    }
    if (appImage) {
      document.querySelectorAll(".download-appimage").forEach((link) => {
        link.href = appImage;
      });
    }
  } catch {}
}

const copyButton = document.querySelector("[data-copy]");
copyButton?.addEventListener("click", async () => {
  const command = document.querySelector("[data-command]")?.textContent?.trim();
  if (!command) {
    return;
  }
  try {
    await navigator.clipboard.writeText(command);
  } catch {
    const range = document.createRange();
    range.selectNodeContents(document.querySelector("[data-command]"));
    const selection = window.getSelection();
    selection.removeAllRanges();
    selection.addRange(range);
  }
  copyButton.textContent = "Copied";
  const toast = document.querySelector(".toast");
  toast?.classList.add("show");
  window.setTimeout(() => {
    copyButton.textContent = "Copy";
    toast?.classList.remove("show");
  }, 1800);
});

refreshRelease();
