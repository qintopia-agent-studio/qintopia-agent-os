export const requiredXiaomanProductionCompletionEvidenceRefs = [
  "docs/plans/active/xiaoman-production-completion-gate.md",
  "tools/deploy/check-xiaoman-production-completion-evidence.mjs",
  "tools/deploy/check-xiaoman-qiwe-group-arrival-confirmation-evidence.mjs",
  "--qiwe-group-arrival-confirmation",
  "xiaoman-production-completion-evidence-v1",
  "owner-retained evidence",
];

const positiveClaimPatterns = [
  /\bXiaoman\b[^\n。；;]*(?:production[- ]complete|fully usable in production|end[- ]to[- ]end production)/i,
  /(?:production[- ]complete|fully usable in production|end[- ]to[- ]end production)[^\n。；;]*\bXiaoman\b/i,
  /小满[^\n。；;]*(?:生产完成|生产可用|生产闭环已完成|生产端到端已完成|生产已跑通)/,
];

const negatedClaimPattern =
  /\b(?:not|never|must not|do not|does not|cannot|can't|is not|isn't|remains? not|missing|blocked|incomplete|not complete|not production[- ]complete)\b|未|不|不能|尚未|缺失|未完成|不可|没有/iu;

export const positiveXiaomanProductionCompletionClaimLines = (text) =>
  String(text || "")
    .replace(/\r\n/g, "\n")
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line)
    .filter((line) => positiveClaimPatterns.some((pattern) => pattern.test(line)))
    .filter((line) => !negatedClaimPattern.test(line));

export const validateXiaomanProductionCompletionClaimBoundary = ({
  pullRequestBody = "",
  changelog = "",
} = {}) => {
  const errors = [];
  const checkedTexts = [
    ["Release Please PR body", pullRequestBody],
    ["CHANGELOG.md", changelog],
  ];
  const claimLines = checkedTexts.flatMap(([source, text]) =>
    positiveXiaomanProductionCompletionClaimLines(text).map((line) => ({
      source,
      line,
    }))
  );

  if (claimLines.length === 0) {
    return errors;
  }

  const combinedReleaseText = `${pullRequestBody}\n${changelog}`;
  const missingRefs = requiredXiaomanProductionCompletionEvidenceRefs.filter(
    (fragment) => !combinedReleaseText.includes(fragment)
  );
  if (missingRefs.length === 0) {
    return errors;
  }

  errors.push(
    [
      "Xiaoman production-complete release claim requires owner-retained completion evidence references.",
      "Claim lines:",
      ...claimLines.map(({ source, line }) => `  - ${source}: ${line}`),
      "Missing references:",
      ...missingRefs.map((fragment) => `  - ${fragment}`),
    ].join("\n")
  );

  return errors;
};
