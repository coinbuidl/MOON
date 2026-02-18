# Security Checklist

1. Never commit real API keys.
2. Keep `.env` and `.env.*` ignored.
3. Use least-privilege API keys for Gemini.
4. Treat archives as sensitive session data; set file permissions appropriately.
5. Rotate keys immediately if exposure is suspected.
6. Audit logs must not include secrets.
7. Use HTTPS-only model endpoints.
