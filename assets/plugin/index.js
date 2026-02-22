function isObject(value) {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function clampInt(value, fallback, min, max) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) {
    return fallback;
  }
  const rounded = Math.floor(parsed);
  return Math.max(min, Math.min(max, rounded));
}

function estimateTokens(text) {
  if (typeof text !== "string" || text.length === 0) {
    return 0;
  }

  const lengthBased = Math.ceil(text.length / 4);
  const words = text.trim() ? text.trim().split(/\s+/).length : 0;
  const wordBased = Math.ceil(words * 1.33);
  const hasCjk = /[\u3040-\u30ff\u3400-\u9fff\uf900-\ufaff]/.test(text);
  if (hasCjk) {
    return Math.max(lengthBased, Math.ceil(text.length * 0.85));
  }
  return Math.max(lengthBased, wordBased);
}

function estimateBytes(text) {
  if (typeof text !== "string") {
    return 0;
  }
  return Buffer.byteLength(text, "utf8");
}

function compactByBudget(text, limits) {
  if (typeof text !== "string") {
    return { text, truncated: false, estimatedTokensBefore: 0, estimatedTokensAfter: 0 };
  }

  const estimatedTokensBefore = estimateTokens(text);
  const withinChar = text.length <= limits.maxChars;
  const withinToken = estimatedTokensBefore <= limits.maxTokens;
  if (withinChar && withinToken) {
    return {
      text,
      truncated: false,
      estimatedTokensBefore,
      estimatedTokensAfter: estimatedTokensBefore,
    };
  }

  const charBudgetFromTokens = Math.max(800, limits.maxTokens * 4);
  const effectiveCharBudget = Math.max(800, Math.min(limits.maxChars, charBudgetFromTokens));

  const omittedTokens = Math.max(0, estimatedTokensBefore - limits.maxTokens);
  const marker =
    `\n\n[MOON truncated ~${omittedTokens} tokens; ` +
    `full payload may be available in details]\n\n`;

  const sliceBudget = Math.max(220, effectiveCharBudget - marker.length);
  let head = Math.max(120, Math.floor(sliceBudget * 0.62));
  let tail = Math.max(80, sliceBudget - head);

  if (head + tail >= text.length) {
    head = Math.max(40, Math.floor(text.length * 0.6));
    tail = Math.max(20, Math.floor(text.length * 0.2));
  }

  const start = text.slice(0, head);
  const end = text.slice(-tail);
  const omittedChars = Math.max(0, text.length - head - tail);
  const compacted = `${start}${marker}[omitted ${omittedChars} chars]\n${end}`;

  return {
    text: compacted,
    truncated: true,
    estimatedTokensBefore,
    estimatedTokensAfter: estimateTokens(compacted),
  };
}

function projectJsonSummary(text) {
  try {
    const parsed = JSON.parse(text);
    if (Array.isArray(parsed)) {
      return JSON.stringify(
        {
          kind: "array",
          length: parsed.length,
          sample: parsed.slice(0, 20),
        },
        null,
        2,
      );
    }
    if (isObject(parsed)) {
      const keys = Object.keys(parsed);
      const sample = {};
      for (const key of keys.slice(0, 20)) {
        const value = parsed[key];
        if (Array.isArray(value)) {
          sample[key] = `[array:${value.length}]`;
        } else if (isObject(value)) {
          sample[key] = `[object:${Object.keys(value).length} keys]`;
        } else {
          sample[key] = value;
        }
      }
      return JSON.stringify(
        {
          kind: "object",
          keyCount: keys.length,
          keys: keys.slice(0, 60),
          sample,
        },
        null,
        2,
      );
    }
  } catch {
    return null;
  }
  return null;
}

const DEFAULT_TOOL_PROFILES = {
  read: { maxTokens: 6000, maxChars: 32000 },
  "message/readMessages": { maxTokens: 5000, maxChars: 28000 },
  "message/searchMessages": { maxTokens: 5000, maxChars: 28000 },
  web_fetch: { maxTokens: 7000, maxChars: 35000 },
  "web.fetch": { maxTokens: 7000, maxChars: 35000 },
};

function resolveLimits(pluginConfig, toolName) {
  const globalMaxTokens = clampInt(pluginConfig.maxTokens, 12000, 500, 500000);
  const globalMaxChars = clampInt(pluginConfig.maxChars, 60000, 1000, 200000);
  const maxRetainedBytes = clampInt(pluginConfig.maxRetainedBytes, 250000, 0, 5000000);

  const profileDefault = isObject(DEFAULT_TOOL_PROFILES[toolName])
    ? DEFAULT_TOOL_PROFILES[toolName]
    : {};
  const toolCfg =
    isObject(pluginConfig.tools) && isObject(pluginConfig.tools[toolName])
      ? pluginConfig.tools[toolName]
      : {};

  const maxTokens = clampInt(
    toolCfg.maxTokens,
    clampInt(profileDefault.maxTokens, globalMaxTokens, 100, 500000),
    100,
    500000,
  );
  const maxChars = clampInt(
    toolCfg.maxChars,
    clampInt(profileDefault.maxChars, globalMaxChars, 200, 200000),
    200,
    200000,
  );

  return { maxTokens, maxChars, maxRetainedBytes };
}

function compactToolResultMessage(message, toolName, pluginConfig) {
  if (!isObject(message)) {
    return message;
  }
  if (message.role !== "toolResult" || !Array.isArray(message.content)) {
    return message;
  }

  const limits = resolveLimits(pluginConfig, String(toolName || ""));
  const namesForJsonProjection = new Set([
    "read",
    "message/readMessages",
    "message/searchMessages",
    "web_fetch",
    "web.fetch",
  ]);

  const nextContent = [];
  const fullTextParts = [];
  let fullTextBytes = 0;
  let mutated = false;
  const strategies = new Set();
  let textBlockCount = 0;
  let compactedBlockCount = 0;
  let totalCharsBefore = 0;
  let totalCharsAfter = 0;
  let totalTokensBefore = 0;
  let totalTokensAfter = 0;

  for (const block of message.content) {
    if (!isObject(block) || block.type !== "text" || typeof block.text !== "string") {
      nextContent.push(block);
      continue;
    }

    textBlockCount += 1;
    const originalText = block.text;
    let workingText = originalText;

    const blockChars = originalText.length;
    const blockTokens = estimateTokens(originalText);
    totalCharsBefore += blockChars;
    totalTokensBefore += blockTokens;

    fullTextParts.push(originalText);
    fullTextBytes += estimateBytes(originalText);

    if (
      namesForJsonProjection.has(String(toolName || "")) &&
      (blockChars > limits.maxChars || blockTokens > limits.maxTokens)
    ) {
      const projected = projectJsonSummary(originalText);
      if (projected) {
        workingText = `[MOON projected JSON summary]\n${projected}`;
        strategies.add("json_projection");
      }
    }

    const compacted = compactByBudget(workingText, limits);
    totalCharsAfter += compacted.text.length;
    totalTokensAfter += compacted.estimatedTokensAfter;

    if (compacted.truncated || workingText !== originalText) {
      mutated = true;
      compactedBlockCount += 1;
      if (compacted.truncated) {
        strategies.add("head_tail_trim");
      }
      nextContent.push({ ...block, text: compacted.text });
    } else {
      nextContent.push(block);
    }
  }

  if (!mutated) {
    return message;
  }

  const details = isObject(message.details) ? { ...message.details } : {};
  const metadata = {
    compactedAt: new Date().toISOString(),
    toolName: toolName || null,
    strategy:
      strategies.size > 0
        ? Array.from(strategies).sort().join("+")
        : "head_tail_trim",
    textBlockCount,
    compactedBlockCount,
    originalTextChars: totalCharsBefore,
    finalTextChars: totalCharsAfter,
    estimatedTokensBefore: totalTokensBefore,
    estimatedTokensAfter: totalTokensAfter,
    maxTokens: limits.maxTokens,
    maxChars: limits.maxChars,
    maxRetainedBytes: limits.maxRetainedBytes,
    retrievalHint:
      "Use the original source/tool call id to refetch full payload if omitted from persisted text.",
  };

  if (limits.maxRetainedBytes > 0 && fullTextBytes <= limits.maxRetainedBytes) {
    metadata.fullText = fullTextParts.join("\n");
    metadata.fullTextRetained = true;
  } else {
    metadata.fullTextRetained = false;
  }

  details.moon = metadata;

  return { ...message, content: nextContent, details };
}

export default {
  id: "MOON",
  register(api) {
    api.on("tool_result_persist", (event, ctx) => {
      const pluginCfg = isObject(api && api.pluginConfig) ? api.pluginConfig : {};
      const toolName = event.toolName || ctx.toolName || "";
      const next = compactToolResultMessage(event.message, toolName, pluginCfg);
      return { message: next };
    });
  },
};
