import bundledTitleKeywords from "../../../assets/keyword-lists/window-title-explicit.json";
import type { TitleKeywordCategory, TitleKeywordMatch } from "./types";

export type BundledKeywordConfidence = "strict" | "ambiguous";

export interface BundledKeyword {
  term: string;
  category: TitleKeywordCategory;
  /** Strict keywords are always enforced; ambiguous ones only when enabled. */
  confidence: BundledKeywordConfidence;
  match: TitleKeywordMatch;
}

export interface BundledKeywordGroup {
  language: string;
  keywords: BundledKeyword[];
}

/** Matches the Rust policy engine's limits for additions to the built-in list. */
export const maxTitleKeywordAdditions = 1024;
export const maxTitleKeywordTermChars = 64;

const categories: readonly string[] = ["organ", "sexual"];
const confidences: readonly string[] = ["strict", "ambiguous"];
const matches: readonly string[] = ["word", "substring", "prefix"];

function invalid(): never {
  throw new Error("内置关键词词库格式不正确");
}

function parseKeyword(value: unknown): BundledKeyword {
  if (typeof value !== "object" || value === null) invalid();
  const { term, category, confidence, match } = value as Record<string, unknown>;
  if (
    typeof term !== "string" ||
    !term.trim() ||
    typeof category !== "string" ||
    !categories.includes(category) ||
    typeof confidence !== "string" ||
    !confidences.includes(confidence) ||
    typeof match !== "string" ||
    !matches.includes(match)
  ) {
    invalid();
  }
  return { term, category: category as TitleKeywordCategory, confidence: confidence as BundledKeywordConfidence, match: match as TitleKeywordMatch };
}

// The Rust policy engine embeds this same file via include_str! at compile time;
// importing it here keeps the console view in sync with the enforced word list.
function loadBundledGroups(): BundledKeywordGroup[] {
  const document = bundledTitleKeywords as { format_version?: unknown; languages?: unknown };
  if (document.format_version !== 2 || !Array.isArray(document.languages)) invalid();
  const groups = document.languages.map((entry): BundledKeywordGroup => {
    if (typeof entry !== "object" || entry === null) invalid();
    const { language, keywords } = entry as Record<string, unknown>;
    if (typeof language !== "string" || !isLanguageCode(language) || !Array.isArray(keywords)) invalid();
    return { language, keywords: keywords.map(parseKeyword) };
  });
  // Largest lists first, so the most common languages lead the view.
  return groups.sort((left, right) => right.keywords.length - left.keywords.length || left.language.localeCompare(right.language));
}

export function isLanguageCode(value: string): boolean {
  return /^[a-z]{2,3}$/.test(value);
}

/** The same normalization the policy engine applies before matching. */
export function normalizeKeyword(value: string): string {
  return value.normalize("NFKC").toLowerCase().trim();
}

/** Languages whose words are not separated by spaces match by substring. */
export function defaultMatchFor(language: string): TitleKeywordMatch {
  return ["zh", "ja", "ko", "th"].includes(language) ? "substring" : "word";
}

export const bundledKeywordGroups: BundledKeywordGroup[] = loadBundledGroups();

export const bundledLanguages: string[] = bundledKeywordGroups.map((group) => group.language);

const allBundled = bundledKeywordGroups.flatMap((group) => group.keywords);
export const bundledKeywordCount: number = allBundled.length;
export const bundledAmbiguousCount: number = allBundled.filter((keyword) => keyword.confidence === "ambiguous").length;
export const bundledStrictCount: number = bundledKeywordCount - bundledAmbiguousCount;

const strictKeys: ReadonlySet<string> = new Set(
  allBundled.filter((keyword) => keyword.confidence === "strict").map((keyword) => `${keyword.match}:${normalizeKeyword(keyword.term)}`),
);

/** Whether a strict built-in keyword already enforces `term` at least as broadly. */
export function coveredByStrictKeyword(term: string, match: TitleKeywordMatch): boolean {
  const normalized = normalizeKeyword(term);
  return strictKeys.has(`${match}:${normalized}`) || strictKeys.has(`substring:${normalized}`);
}
