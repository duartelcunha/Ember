import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { Button } from "@/components/ui/button";

type Status = "idle" | "checking" | "up-to-date" | "downloading" | "ready" | "error";

/** Check-for-updates control: check -> download+install -> relaunch. Signed + verified by the updater plugin. */
export function UpdateChecker() {
  const [version, setVersion] = useState("");
  const [status, setStatus] = useState<Status>("idle");
  const [progress, setProgress] = useState(0);
  const [newVersion, setNewVersion] = useState<string | null>(null);

  useEffect(() => {
    getVersion()
      .then(setVersion)
      .catch(() => {
        /* outside Tauri */
      });
  }, []);

  const run = async () => {
    setStatus("checking");
    try {
      const update = await check();
      if (!update) {
        setStatus("up-to-date");
        return;
      }
      setNewVersion(update.version);
      setStatus("downloading");
      let total = 0;
      let downloaded = 0;
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? 0;
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          setProgress(total ? Math.min(100, Math.round((downloaded / total) * 100)) : 0);
        }
      });
      setStatus("ready");
    } catch {
      setStatus("error");
    }
  };

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center justify-between">
        <span className="text-sm text-fg-muted">
          {version ? `Version ${version}` : "Version unknown"}
        </span>
        {status !== "downloading" && status !== "ready" && (
          <Button variant="ghost" size="sm" onClick={run} disabled={status === "checking"}>
            {status === "checking" ? "Checking…" : "Check for updates"}
          </Button>
        )}
      </div>
      {status === "up-to-date" && (
        <p className="text-xs text-success">You're on the latest version.</p>
      )}
      {status === "downloading" && (
        <div className="flex flex-col gap-1.5">
          <p className="text-xs text-fg-muted">
            Downloading {newVersion}… {progress}%
          </p>
          <div className="h-1.5 w-full overflow-hidden rounded-full bg-surface-3">
            <div
              className="h-full rounded-full bg-accent transition-[width] duration-200"
              style={{ width: `${progress}%` }}
            />
          </div>
        </div>
      )}
      {status === "ready" && (
        <div className="flex items-center justify-between">
          <p className="text-xs text-success">Update {newVersion} installed.</p>
          <Button variant="primary" size="sm" onClick={() => relaunch()}>
            Restart now
          </Button>
        </div>
      )}
      {status === "error" && (
        <p className="text-xs text-error">Couldn't check for updates. Try again later.</p>
      )}
    </div>
  );
}
