/**
 * Script rules the app (`script/dart`) and the web must apply alike: the counts are sent to the
 * server, which stores them as given, and word indices address slide mappings. Both are pinned
 * to `script/vectors`.
 */

/**
 * A sentence terminator plus the trailing quotes/brackets that still count as part of the
 * ending. The web's teleprompter breaks rendered lines on it, so counting uses the same rule.
 */
export const SENTENCE_END_SOURCE = "[.?!…。！？][\")'’”\\]】』」]*";

const SENTENCE_BOUNDARY = new RegExp(`(?<=${SENTENCE_END_SOURCE})\\s+|\\n+`);

/** Words as the server defines them: split on `\s+`, empty pieces dropped. */
export function countScriptTokens(script: string): number {
  return script.split(/\s+/).filter((token) => token !== "").length;
}

/** Sentences, for `scriptLineCount`: a break after a sentence end, or a line break. */
export function countScriptLines(script: string): number {
  return script.split(SENTENCE_BOUNDARY).filter((line) => line.trim() !== "").length;
}
