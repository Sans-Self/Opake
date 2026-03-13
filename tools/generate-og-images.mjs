#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import { writeFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";

const PUBLIC = join(import.meta.dirname, "../web/public");
const OG_DIR = join(PUBLIC, "og");
mkdirSync(OG_DIR, { recursive: true });

const BG = "#f4f0e8";
const TEXT = "#1C1408";
const MUTED = "#5C4A2E";
const GOLD = "#9A7840";
const WIDTH = 1200;
const HEIGHT = 630;

const LOGO_MARK_LG = `
  <rect x="0" y="0" width="90" height="90" rx="7" fill="${GOLD}" fill-opacity="0.7" />
  <rect x="24" y="24" width="90" height="90" rx="7" fill="${GOLD}" fill-opacity="0.2" stroke="${GOLD}" stroke-opacity="0.45" stroke-width="3.5" />
`;

const LOGO_MARK_SM = `
  <rect x="0" y="0" width="60" height="60" rx="5" fill="${GOLD}" fill-opacity="0.7" />
  <rect x="16" y="16" width="60" height="60" rx="5" fill="${GOLD}" fill-opacity="0.2" stroke="${GOLD}" stroke-opacity="0.45" stroke-width="2.5" />
`;

function escapeXml(s) {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

function wrapText(text, maxChars) {
  const words = text.split(" ");
  const lines = [];
  let current = "";
  for (const word of words) {
    if (current.length + word.length + 1 > maxChars) {
      lines.push(current);
      current = word;
    } else {
      current = current ? `${current} ${word}` : word;
    }
  }
  if (current) lines.push(current);
  return lines;
}

function baseSvg() {
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${WIDTH}" height="${HEIGHT}" viewBox="0 0 ${WIDTH} ${HEIGHT}">
  <rect width="${WIDTH}" height="${HEIGHT}" fill="${BG}" />
  <g transform="translate(${(WIDTH - 114) / 2}, 110)">
    ${LOGO_MARK_LG}
  </g>
  <text x="${WIDTH / 2}" y="330" text-anchor="middle"
    font-family="Cormorant Garamond, Georgia, serif" font-size="120" font-weight="500"
    letter-spacing="0.05em" fill="${TEXT}">Opake</text>
  <text x="${WIDTH / 2}" y="400" text-anchor="middle"
    font-family="Inter, Helvetica, sans-serif" font-size="34" fill="${MUTED}">Your data, freely shared, privately kept</text>
</svg>`;
}

function docSvg(title, description) {
  const titleLines = wrapText(escapeXml(title), 22);
  const descLines = wrapText(escapeXml(description), 45);

  const titleY = 290;
  const titleMarkup = titleLines
    .map((line, i) => `<tspan x="100" dy="${i === 0 ? 0 : 95}">${line}</tspan>`)
    .join("");
  const descStartY = titleY + titleLines.length * 95 + 36;
  const descMarkup = descLines
    .map((line, i) => `<tspan x="100" dy="${i === 0 ? 0 : 42}">${line}</tspan>`)
    .join("");

  return `<svg xmlns="http://www.w3.org/2000/svg" width="${WIDTH}" height="${HEIGHT}" viewBox="0 0 ${WIDTH} ${HEIGHT}">
  <rect width="${WIDTH}" height="${HEIGHT}" fill="${BG}" />
  <g transform="translate(100, 70)">
    ${LOGO_MARK_SM}
  </g>
  <text x="186" y="115"
    font-family="Cormorant Garamond, Georgia, serif" font-size="42" font-weight="500"
    letter-spacing="0.05em" fill="${TEXT}">Opake</text>
  <text y="${titleY}"
    font-family="Cormorant Garamond, Georgia, serif" font-size="84" font-weight="400"
    fill="${TEXT}">${titleMarkup}</text>
  <text y="${descStartY}"
    font-family="Inter, Helvetica, sans-serif" font-size="32" fill="${MUTED}">${descMarkup}</text>
</svg>`;
}

const DOCS = [
  { slug: "getting-started", title: "Getting Started", description: "Set up your cabinet, create your first encrypted file, and explore the interface." },
  { slug: "at-protocol", title: "AT Protocol", description: "The open standard powering Opake — identity, data portability, and federation." },
  { slug: "encryption-keys", title: "Encryption & Keys", description: "How end-to-end encryption works in Opake and how your keys are managed." },
  { slug: "sharing-dids", title: "Sharing & DIDs", description: "Share files using decentralised identifiers without a central authority." },
  { slug: "keyrings", title: "Keyrings & Groups", description: "Manage secure group sharing for families, teams, and research groups." },
  { slug: "pairing", title: "Multi-Device Magic", description: "Securely transfer your identity keypair to new devices using your PDS as a relay." },
  { slug: "cli", title: "The CLI Manual", description: "Complete command reference for the Opake CLI — identity, files, sharing, and more." },
  { slug: "glossary", title: "Glossary", description: "A quick-hit reference for the terminology and acronyms we use in Opake." },
  { slug: "faq", title: "FAQ", description: "Common questions about privacy, security, and how Opake compares to alternatives." },
];

function renderPng(svgContent, outputPath) {
  const tmpSvg = `${outputPath}.tmp.svg`;
  writeFileSync(tmpSvg, svgContent);
  execFileSync("npx", ["sharp-cli", "-i", tmpSvg, "-o", outputPath, "resize", String(WIDTH), String(HEIGHT)], {
    cwd: join(import.meta.dirname, "../web"),
    stdio: "pipe",
  });
  execFileSync("rm", [tmpSvg]);
}

console.log("Generating base OG image...");
renderPng(baseSvg(), join(OG_DIR, "default.png"));

for (const doc of DOCS) {
  console.log(`Generating OG image for ${doc.slug}...`);
  renderPng(docSvg(doc.title, doc.description), join(OG_DIR, `${doc.slug}.png`));
}

console.log("Done!");
