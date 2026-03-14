use std::io::{BufRead, Write};

use anyhow::{Context, Result};

/// Yes/No confirmation. Returns `true` only on case-insensitive "y".
/// Prints prompt to stderr (UX chrome, not pipeable output).
pub fn confirm(prompt: &str) -> Result<bool> {
    confirm_from(prompt, &mut std::io::stdin().lock(), &mut std::io::stderr())
}

fn confirm_from(prompt: &str, reader: &mut impl BufRead, writer: &mut impl Write) -> Result<bool> {
    write!(writer, "{prompt} [y/N] ")?;
    writer.flush()?;

    let mut input = String::new();
    reader
        .read_line(&mut input)
        .context("failed to read stdin")?;

    Ok(input.trim().eq_ignore_ascii_case("y"))
}

/// Scary confirmation requiring an exact phrase match. Bails on mismatch.
/// Prints warning lines to stdout, prompt to stderr.
pub fn confirm_exact(warning: &str, phrase: &str) -> Result<()> {
    confirm_exact_from(
        warning,
        phrase,
        &mut std::io::stdin().lock(),
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    )
}

fn confirm_exact_from(
    warning: &str,
    phrase: &str,
    reader: &mut impl BufRead,
    info_writer: &mut impl Write,
    prompt_writer: &mut impl Write,
) -> Result<()> {
    writeln!(info_writer, "{warning}")?;
    writeln!(info_writer, "Type exactly: {phrase}")?;
    write!(prompt_writer, "> ")?;
    prompt_writer.flush()?;

    let mut input = String::new();
    reader
        .read_line(&mut input)
        .context("failed to read stdin")?;

    if input.trim() != phrase {
        anyhow::bail!("Cancelled.");
    }

    Ok(())
}

/// Generic trimmed line input. Prints prompt to stderr, returns trimmed string.
pub fn input(prompt: &str) -> Result<String> {
    input_from(prompt, &mut std::io::stdin().lock(), &mut std::io::stderr())
}

fn input_from(prompt: &str, reader: &mut impl BufRead, writer: &mut impl Write) -> Result<String> {
    write!(writer, "{prompt}")?;
    writer.flush()?;

    let mut input = String::new();
    reader
        .read_line(&mut input)
        .context("failed to read stdin")?;

    Ok(input.trim().to_string())
}

/// Input with a default value shown in brackets. Empty input returns the default.
pub fn input_with_default(prompt: &str, default: &str) -> Result<String> {
    input_with_default_from(
        prompt,
        default,
        &mut std::io::stdin().lock(),
        &mut std::io::stderr(),
    )
}

fn input_with_default_from(
    prompt: &str,
    default: &str,
    reader: &mut impl BufRead,
    writer: &mut impl Write,
) -> Result<String> {
    write!(writer, "{prompt} [{default}] ")?;
    writer.flush()?;

    let mut input = String::new();
    reader
        .read_line(&mut input)
        .context("failed to read stdin")?;

    let trimmed = input.trim();
    if trimmed.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(trimmed.to_string())
    }
}

#[cfg(test)]
#[path = "prompt_tests.rs"]
mod tests;
