import { Button, Notice } from "@veronica/ui";
import { Code2, Sparkles } from "lucide-react";
import { type ReactNode, useEffect, useId, useState } from "react";
import veronicaLogo from "../assets/veronica-logo.png";
import { invoke, ipc } from "../lib/ipc";
import { chooseProfile, type Detection, type Profile } from "../lib/onboarding";
import "./Onboarding.css";

const CHOICES: { profile: Profile; title: string; detail: string; icon: ReactNode }[] = [
  {
    profile: "simple",
    title: "Simple",
    detail: "Music, calendar, clipboard, emoji and a calm view of this computer.",
    icon: <Sparkles />,
  },
  {
    profile: "developer",
    title: "Developer",
    detail: "Agent usage, machines, ports and your developer workspace on top.",
    icon: <Code2 />,
  },
];

function found(detection: Detection | null): string | null {
  const names = [...(detection?.editors ?? []), ...(detection?.agents ?? [])];
  if (names.length === 0) {
    return null;
  }
  return `Suggested because ${names.slice(0, 3).join(", ")}${names.length > 3 ? " and more" : ""} ${names.length === 1 ? "is" : "are"} on this computer.`;
}

export function OnboardingPage({ onDone }: { onDone: () => void }) {
  const [detection, setDetection] = useState<Detection | null>(null);
  const [profile, setProfile] = useState<Profile>("simple");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const name = useId();

  useEffect(() => {
    let live = true;
    void ipc.extOnboardingDetect().then(
      (result) => {
        if (live) {
          setDetection(result);
          setProfile(result.suggested);
        }
      },
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, []);

  const finish = async () => {
    setBusy(true);
    setError(null);
    try {
      await chooseProfile(invoke, profile, []);
      onDone();
    } catch (reason) {
      setError(String(reason));
      setBusy(false);
    }
  };

  const reason = found(detection);

  return (
    <main className="onboarding">
      <div className="onboarding-panel">
        <img className="onboarding-mark" src={veronicaLogo} alt="" aria-hidden="true" />
        <h1 className="onboarding-title">How will you use Veronica?</h1>
        <p className="onboarding-lead">
          Pick a starting set of extensions. You can add or remove any of them later in the Library.
        </p>
        <fieldset className="onboarding-choices">
          <legend className="onboarding-legend">Profile</legend>
          {CHOICES.map((choice) => (
            <label key={choice.profile} className="onboarding-choice">
              <input
                type="radio"
                name={name}
                value={choice.profile}
                checked={profile === choice.profile}
                onChange={() => setProfile(choice.profile)}
              />
              <span className="onboarding-choice-icon" aria-hidden="true">
                {choice.icon}
              </span>
              <span className="onboarding-choice-text">
                <span className="onboarding-choice-title">{choice.title}</span>
                <span className="onboarding-choice-detail">{choice.detail}</span>
                {choice.profile === "developer" && reason !== null && (
                  <span className="onboarding-choice-reason">{reason}</span>
                )}
              </span>
            </label>
          ))}
        </fieldset>
        {error !== null && (
          <Notice tone="critical" title="That did not save">
            {error}
          </Notice>
        )}
        <Button variant="primary" busy={busy} onClick={() => void finish()}>
          Continue
        </Button>
      </div>
    </main>
  );
}
