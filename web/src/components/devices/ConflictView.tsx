import { useState } from "react";
import { useAuthStore } from "@/stores/auth";
import { ArrowsLeftRightIcon, KeyIcon, WarningIcon, AmbulanceIcon } from "@phosphor-icons/react";
import { DestructiveConfirmation } from "@/components/DestructiveConfirmation";
import { ChoiceButton } from "./ChoiceButton";
import { PageHeader } from "./PageHeader";

export function ConflictView() {
  const [confirming, setConfirming] = useState(false);

  if (confirming) {
    return (
      <div className="flex flex-col items-center gap-6 text-center">
        <PageHeader
          icon={WarningIcon}
          iconClassName="text-error"
          title="This will destroy your current key"
          description="Files encrypted with the old key won't be accessible."
        />
        <DestructiveConfirmation
          phrase="This will make my old data unusable and I am okay with that"
          onConfirm={() => void useAuthStore.getState().generateAndPublishIdentity()}
        />
        <button
          onClick={() => setConfirming(false)}
          className="text-base-content/50 hover:text-base-content/70 cursor-pointer text-sm"
        >
          Go back
        </button>
      </div>
    );
  }

  return (
    <div className="flex flex-col items-center gap-6 text-center">
      <PageHeader
        icon={WarningIcon}
        title="This device is out of sync"
        description="The key on this device doesn't match the one your files were encrypted with. Pick how you'd like to fix it."
      />

      <div className="flex w-full flex-wrap justify-center-safe gap-4">
        <ChoiceButton
          as="Button"
          onClick={() => setConfirming(true)}
          icon={KeyIcon}
          title="Start fresh"
          description="Create a new key. Files encrypted with the old key won't be accessible."
        />

        <ChoiceButton
          as="Link"
          to="/devices/pair/request"
          icon={ArrowsLeftRightIcon}
          title="Sync from another device"
          description="Use a device that can already open your files to copy the key here."
        />

        <ChoiceButton
          as="Button"
          disabled
          icon={AmbulanceIcon}
          title="Use your recovery phrase"
          description="Enter the 24 words you saved when you first set up. (coming soon)"
        />
      </div>
    </div>
  );
}
