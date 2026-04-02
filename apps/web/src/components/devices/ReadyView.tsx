import { useAuthStore } from "@/stores/auth";
import { ArrowsLeftRightIcon } from "@phosphor-icons/react";
import { Link } from "@tanstack/react-router";
import { ChoiceButton } from "./ChoiceButton";
import { PageHeader } from "./PageHeader";

export function ReadyView() {
  return (
    <div className="flex flex-col items-center gap-6 text-center">
      <PageHeader
        title="You're all set"
        description="This device can encrypt and decrypt your files. You can also use it to set up other devices."
      />

      <ChoiceButton
        as="Link"
        to="/devices/pair/accept"
        icon={ArrowsLeftRightIcon}
        title="Set up another device"
        description="Share your key with a phone, tablet, or another computer."
      />

      <div className="flex flex-col items-center gap-2">
        <Link to="/cabinet" className="text-base-content/50 hover:text-base-content/70 text-sm">
          Back to cabinet
        </Link>
        <button
          onClick={() =>
            void useAuthStore
              .getState()
              .logout()
              .then(() => {
                window.location.href = "/devices";
              })
          }
          className="text-error/60 hover:text-error/80 text-sm"
        >
          Log out
        </button>
      </div>
    </div>
  );
}
