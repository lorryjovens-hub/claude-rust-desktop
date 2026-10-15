import { describe, expect, it } from 'vitest';
import { formatChatStreamError, parseBadRequest, readChatResponseError } from './chatErrors';

describe('bad request details', () => {
  it.each(['HTTP 400\n{"detail":"Unsupported model"}', 'OpenAI API error 400 Bad Request: unsupported', '400 {"error":"invalid"}', '{"error":{"status":400,"message":"unsupported"}}'])('recognizes status in %s', error => {
    expect(parseBadRequest(error)?.status).toBe(400);
  });
  it.each(['Used 400 tokens', 'HTTP 401\nUnauthorized', 'HTTP 500\nServer error', 'model-400 is unavailable'])('does not classify unrelated numbers: %s', error => {
    expect(parseBadRequest(error)).toBeNull();
  });
  it('retains JSON, text and empty HTTP response bodies', async () => {
    for (const body of ['{"detail":"Unsupported model"}', '<html>Bad request</html>', '']) {
      const error = parseBadRequest(await readChatResponseError(new Response(body, { status: 400 })));
      expect(error).toEqual({ status: 400, detail: body || 'Bad Request' });
    }
  });
  it('retains structured stream errors and status', () => {
    expect(parseBadRequest(formatChatStreamError({ status: 400, error: { message: 'Unsupported model' } })))
      .toEqual({ status: 400, detail: '{"message":"Unsupported model"}' });
  });
});
