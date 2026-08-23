import bundledTitleKeywords from "../../../assets/keyword-lists/window-title-explicit.json";

export interface BundledKeywordGroup {
  language: string;
  keywords: string[];
}

// The Rust policy engine embeds this same file via include_str! at compile time;
// importing it here keeps the console view in sync with the enforced word list.
function loadBundledGroups(): BundledKeywordGroup[] {
  const document = bundledTitleKeywords as { format_version?: unknown; languages?: unknown };
  if (document.format_version !== 1 || !Array.isArray(document.languages)) {
    throw new Error("内置关键词词库格式不正确");
  }
  const groups: BundledKeywordGroup[] = [];
  for (const entry of document.languages) {
    if (
      typeof entry !== "object" ||
      entry === null ||
      typeof (entry as { language?: unknown }).language !== "string" ||
      !Array.isArray((entry as { keywords?: unknown }).keywords)
    ) {
      throw new Error("内置关键词词库格式不正确");
    }
    const keywords = (entry as { keywords: unknown[] }).keywords.filter(
      (keyword): keyword is string => typeof keyword === "string",
    );
    groups.push({ language: (entry as { language: string }).language, keywords });
  }
  return groups;
}

export const bundledKeywordGroups: BundledKeywordGroup[] = loadBundledGroups();

export const bundledKeywordCount: number = bundledKeywordGroups.reduce(
  (total, group) => total + group.keywords.length,
  0,
);
