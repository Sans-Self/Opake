use clap::Args;
use clap_complete::Shell;

/// Generate shell completion scripts
///
/// Outputs a completion script for the specified shell to stdout.
/// Redirect the output to the appropriate file for your shell.
#[derive(Args)]
#[command(after_help = "\
Install completions:
  bash:  opake completions bash > ~/.local/share/bash-completion/completions/opake
  zsh:   opake completions zsh > ~/.zfunc/_opake
  fish:  opake completions fish > ~/.config/fish/completions/opake.fish")]
pub struct CompletionsCommand {
    /// Shell to generate completions for
    shell: Shell,
}

impl CompletionsCommand {
    pub fn run(self, cmd: &mut clap::Command) {
        clap_complete::generate(self.shell, cmd, "opake", &mut std::io::stdout());
    }
}
