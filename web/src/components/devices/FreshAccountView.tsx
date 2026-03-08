import { useAuthStore } from "@/stores/auth";
import { KeyIcon } from "@phosphor-icons/react";
import { ChoiceButton } from "./ChoiceButton";
import { PageHeader } from "./PageHeader";

export function FreshAccountView() {
  return (
    <div className="flex flex-col items-center gap-6 text-center">
      <PageHeader
        title="Welcome to Opake"
        description="To keep your files private, Opake needs to create an encryption key for this device."
      />
      <ChoiceButton
        as="Button"
        onClick={() => void useAuthStore.getState().generateAndPublishIdentity()}
        icon={KeyIcon}
        title="Create my key"
        description="This only takes a moment."
      />
    </div>
  );
}
