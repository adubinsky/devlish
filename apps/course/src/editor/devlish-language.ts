import { StreamLanguage, StringStream } from "@codemirror/language";

interface DevlishState {
  inString: boolean;
}

const controlKeywords =
  /^(If|Otherwise|For each|While|Until|Try|Break|Continue)\b/;

const declarationKeywords = /^(Set|Load|Import|Export)\b/;

const actionKeywords = /^(Print|Fail with|Require|Expect|Checkpoint|Ask)\b/;

const operatorKeywords =
  /^(must contain|must equal|must be present|must be missing|must match|must be one of|equals|contains|is greater than|is less than|is at least|is at most|is not|is|plus|minus|times|divided by|and|or|not|in|respond with)\b/;

const builtinFunctions =
  /^(count of|first of|last of|unique of|flatten of|minimum of|maximum of|sum of|average of|reverse of|sort of|find where|filter where|reject where|any where|all where|partition where|group by|index by|take|drop|zip|chunk|union of|intersection of|difference of|map transform|pluck|uppercase of|lowercase of|trim of|normalize whitespace of|slugify of|title case of|sentence case of|words of|contains text|starts with text|ends with text|date parse|date add days|days between|business days between|length of|round of|abs of|absolute value of|replace|split|join|item|slice|keys of|values of|entries of|has fields|matches shape|type of)\b/;

const constants = /^(true|false|nil|nothing)\b/;

const devlishLanguageDef = StreamLanguage.define<DevlishState>({
  startState(): DevlishState {
    return { inString: false };
  },

  token(stream: StringStream, state: DevlishState): string | null {
    // Continue string
    if (state.inString) {
      while (!stream.eol()) {
        const ch = stream.next();
        if (ch === "\\") {
          stream.next(); // skip escaped char
        } else if (ch === '"') {
          state.inString = false;
          return "string";
        }
      }
      return "string";
    }

    // Skip whitespace
    if (stream.eatSpace()) return null;

    // Comments
    if (stream.match(/^#.*/)) return "comment";

    // Strings
    if (stream.peek() === '"') {
      stream.next();
      state.inString = true;
      while (!stream.eol()) {
        const ch = stream.next();
        if (ch === "\\") {
          stream.next();
        } else if (ch === '"') {
          state.inString = false;
          return "string";
        }
      }
      return "string";
    }

    // Numbers
    if (stream.match(/^\d+(\.\d+)?/)) return "number";

    // Multi-word matches need to be tried before single-word ones.
    // We save position and try each pattern.
    const remaining = stream.string.slice(stream.pos);

    if (controlKeywords.test(remaining)) {
      const m = remaining.match(controlKeywords)!;
      for (let i = 0; i < m[0].length; i++) stream.next();
      return "keyword";
    }

    if (declarationKeywords.test(remaining)) {
      const m = remaining.match(declarationKeywords)!;
      for (let i = 0; i < m[0].length; i++) stream.next();
      return "keyword";
    }

    if (actionKeywords.test(remaining)) {
      const m = remaining.match(actionKeywords)!;
      for (let i = 0; i < m[0].length; i++) stream.next();
      return "keyword";
    }

    if (builtinFunctions.test(remaining)) {
      const m = remaining.match(builtinFunctions)!;
      for (let i = 0; i < m[0].length; i++) stream.next();
      return "variableName.special";
    }

    if (operatorKeywords.test(remaining)) {
      const m = remaining.match(operatorKeywords)!;
      for (let i = 0; i < m[0].length; i++) stream.next();
      return "operatorKeyword";
    }

    if (constants.test(remaining)) {
      const m = remaining.match(constants)!;
      for (let i = 0; i < m[0].length; i++) stream.next();
      return "bool";
    }

    // Default: consume one word or character
    if (stream.match(/^\w+/)) return null;
    stream.next();
    return null;
  },
});

export { devlishLanguageDef };
