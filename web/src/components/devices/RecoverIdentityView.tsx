import { ArrowsLeftRightIcon, AmbulanceIcon } from "@phosphor-icons/react";
import { ChoiceButton } from "./ChoiceButton";
import { PageHeader } from "./PageHeader";

export function RecoverIdentityView() {
  return (
    <div className="flex flex-col items-center gap-6 text-center">
      <PageHeader
        title="Welcome back"
        description="You're already set up on another device. Bring your key here to access your files."
      />

      <div className="flex w-full flex-wrap justify-center-safe gap-4">
        <ChoiceButton
          as="Link"
          to="/devices/pair/request"
          icon={ArrowsLeftRightIcon}
          title="Copy from another device"
          description="Open Opake on a device you're already using and approve the transfer."
        />

        <ChoiceButton
          as="Button"
          disabled
          icon={AmbulanceIcon}
          title="Use your recovery phrase"
          description="[coming v0.2.0] Enter the 24 words you saved when you first set up."
        />
      </div>
    </div>
  );
}
