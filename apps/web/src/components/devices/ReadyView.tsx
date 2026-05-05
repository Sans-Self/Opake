import { useAuthStore } from "@/stores/auth";
import { ArrowsLeftRightIcon, FolderIcon, SignOutIcon } from "@phosphor-icons/react";
import { ChoiceButton } from "./ChoiceButton";
import { PageHeader } from "./PageHeader";

export function ReadyView() {
  const handleLogout = () => {
    void useAuthStore
      .getState()
      .logout()
      .then(() => {
        window.location.href = "/devices";
      });
  };

  return (
    <div className="flex flex-col items-center gap-6 text-center">
      <PageHeader
        title="You're all set"
        description="This device can encrypt and decrypt your files. You can also use it to set up other devices."
      />

      <div className="flex w-full flex-wrap justify-center-safe gap-4">
        <ChoiceButton
          as="Link"
          to="/cabinet/files"
          icon={FolderIcon}
          title="Open your cabinet"
          description="Jump straight to your encrypted files."
        />
        <ChoiceButton
          as="Link"
          to="/devices/pair/accept"
          icon={ArrowsLeftRightIcon}
          title="Set up another device"
          description="Share your key with a phone, tablet, or another computer."
        />
        <ChoiceButton
          as="Button"
          onClick={handleLogout}
          icon={SignOutIcon}
          title="Log out"
          description="Sign out of this device. You can sign back in any time."
          className="border-error/30 hover:border-error/60"
        />
      </div>
    </div>
  );
}
