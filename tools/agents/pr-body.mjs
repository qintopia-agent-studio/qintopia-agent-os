export const requiredSections = [
  "Summary",
  "Planning",
  "Domain",
  "Validation",
  "Production Boundary",
  "Architecture / Tooling Boundary",
  "Changelog",
];

export const templateBodyMarkers = [
  "## Summary\n\n## Planning",
  "Commands run:\n\n```text\n\n```",
  "Branch:\n\n## Domain",
  "Notes:\n\n## Architecture / Tooling Boundary",
];

const sectionPattern = (name) =>
  new RegExp(`(^|\\n)## ${name}\\n([\\s\\S]*?)(?=\\n## |$)`);

export const sectionBody = (body, name) => {
  const match = String(body || "").match(sectionPattern(name));
  return match ? match[2].trim() : "";
};

export const checkedCount = (text) => {
  const matches = String(text || "").match(/- \[[xX]\] /g);
  return matches ? matches.length : 0;
};

export const fencedTextBlock = (text) => {
  const match = String(text || "").match(/```text\n([\s\S]*?)\n```/);
  return match ? match[1].trim() : "";
};

export const validatePrBody = (body) => {
  const errors = [];
  const text = String(body || "").replace(/\r\n/g, "\n");

  if (!text.trim()) {
    errors.push("PR body is empty");
    return errors;
  }

  for (const section of requiredSections) {
    if (!new RegExp(`(^|\\n)## ${section}(\\n|$)`).test(text)) {
      errors.push(`PR body missing section: ${section}`);
    }
  }

  for (const marker of templateBodyMarkers) {
    if (text.includes(marker)) {
      errors.push("PR body still contains empty template placeholders");
      break;
    }
  }

  if (!sectionBody(text, "Summary")) {
    errors.push("Summary must be filled");
  }

  const planning = sectionBody(text, "Planning");
  if (checkedCount(planning) === 0) {
    errors.push("Planning must include at least one checked item");
  }

  const domain = sectionBody(text, "Domain");
  if (checkedCount(domain) === 0) {
    errors.push("Domain must include at least one checked item");
  }

  const validation = sectionBody(text, "Validation");
  if (!fencedTextBlock(validation)) {
    errors.push("Validation commands must be filled");
  }

  const productionBoundary = sectionBody(text, "Production Boundary");
  if (checkedCount(productionBoundary) === 0) {
    errors.push("Production Boundary must include at least one checked item");
  }

  const architectureBoundary = sectionBody(text, "Architecture / Tooling Boundary");
  if (checkedCount(architectureBoundary) === 0) {
    errors.push(
      "Architecture / Tooling Boundary must include at least one checked item"
    );
  }

  const changelog = sectionBody(text, "Changelog");
  if (checkedCount(changelog) === 0) {
    errors.push("Changelog must include at least one checked item");
  }

  return errors;
};
