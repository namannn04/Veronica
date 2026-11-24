import type { ToolInstallOutcome } from "./types";

export type InstallNotice = {
  tone: "good" | "warn";
  text: string;
};

/** Turn every non-error install outcome into honest, actionable UI copy. */
export function installNotice(
  displayName: string,
  outcome: ToolInstallOutcome,
): InstallNotice {
  if (outcome.outcome === "installed") {
    return {
      tone: "good",
      text: `Installed ${displayName} ${outcome.version} at ${outcome.path}.`,
    };
  }
  if (outcome.outcome === "alreadyInstalled") {
    return {
      tone: "good",
      text: `${displayName} ${outcome.version} is already installed at ${outcome.path}.`,
    };
  }
  return {
    tone: "warn",
    text: `${outcome.reason} ${outcome.command ?? outcome.instruction}`,
  };
}
