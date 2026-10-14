import http from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL(".", import.meta.url));
const port = Number(process.env.PORT || 4173);
const apiKey = process.env.ANTHROPIC_API_KEY;
const model = process.env.CLAUDE_MODEL || "claude-sonnet-4-5";

const mime = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".svg": "image/svg+xml",
};

function json(res, status, value) {
  res.writeHead(status, { "content-type": "application/json; charset=utf-8" });
  res.end(JSON.stringify(value));
}

async function readBody(req) {
  let body = "";
  for await (const chunk of req) {
    body += chunk;
    if (body.length > 250_000) throw new Error("Request is too large.");
  }
  return JSON.parse(body || "{}");
}

const systemPrompt = `You are an expert writing editor. Review the user's text for grammar, spelling, punctuation, clarity, concision, tone, flow, and readability. Preserve the author's meaning and voice. Return ONLY valid JSON with this exact shape:
{
  "score": 0-100,
  "summary": "one concise overall assessment",
  "suggestions": [
    {
      "id": "unique string",
      "type": "grammar|spelling|punctuation|clarity|style|tone",
      "original": "exact substring copied from the user's text",
      "replacement": "replacement text",
      "explanation": "short, useful rationale",
      "severity": "high|medium|low"
    }
  ]
}
Include at most 12 high-value suggestions. Never invent an original substring that does not occur verbatim in the input. If the writing is already strong, return an empty suggestions array.`;

async function analyzeWithClaude(text, goals) {
  if (!apiKey) throw new Error("ANTHROPIC_API_KEY is not configured on the server.");

  const response = await fetch("https://api.anthropic.com/v1/messages", {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "x-api-key": apiKey,
      "anthropic-version": "2023-06-01",
    },
    body: JSON.stringify({
      model,
      max_tokens: 2600,
      temperature: 0.2,
      system: systemPrompt,
      messages: [{
        role: "user",
        content: `Editing goals: ${goals || "Professional, clear, concise, and natural"}\n\nText to review:\n---\n${text}\n---`,
      }],
    }),
  });

  const payload = await response.json();
  if (!response.ok) throw new Error(payload?.error?.message || "Claude API request failed.");
  const content = payload.content?.find((item) => item.type === "text")?.text || "";
  const clean = content.replace(/^```json\s*/i, "").replace(/\s*```$/i, "");
  return JSON.parse(clean);
}

const server = http.createServer(async (req, res) => {
  try {
    const url = new URL(req.url, `http://${req.headers.host}`);

    if (req.method === "POST" && url.pathname === "/api/analyze") {
      const { text, goals } = await readBody(req);
      if (typeof text !== "string" || !text.trim()) return json(res, 400, { error: "Please enter some text first." });
      if (text.length > 30_000) return json(res, 400, { error: "Text must be under 30,000 characters." });
      return json(res, 200, await analyzeWithClaude(text, goals));
    }

    if (req.method !== "GET") return json(res, 405, { error: "Method not allowed" });
    const requested = url.pathname === "/" ? "index.html" : url.pathname.slice(1);
    const safePath = normalize(requested).replace(/^(\.\.[/\\])+/, "");
    const filePath = join(root, safePath);
    const data = await readFile(filePath);
    res.writeHead(200, { "content-type": mime[extname(filePath)] || "application/octet-stream" });
    res.end(data);
  } catch (error) {
    if (error?.code === "ENOENT") return json(res, 404, { error: "Not found" });
    console.error(error);
    json(res, 500, { error: error instanceof Error ? error.message : "Unexpected server error" });
  }
});

server.listen(port, () => {
  console.log(`Prose is running at http://localhost:${port}`);
  if (!apiKey) console.log("Demo mode UI is ready. Set ANTHROPIC_API_KEY to enable live Claude analysis.");
});
