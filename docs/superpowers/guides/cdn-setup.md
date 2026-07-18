# PicHost — CDN Setup Guide

PicHost serves images with `Cache-Control: public, max-age=31536000, immutable` headers
and Nginx proxy caching (`/u/*`, `/t/*`). Adding a CDN layer provides edge caching and DDoS protection.

## Cloudflare (Free Tier)

1. **Add your domain to Cloudflare** — follow their DNS setup wizard.

2. **Configure DNS**: Point your domain (`pichost.example.com`) to your server's IP
   with the orange cloud (proxied) enabled.

3. **Cache Rules** (Dashboard → Caching → Cache Rules):
   - **Image files**: URI path starts with `/u/` → Cache: Eligible, Edge TTL: 7 days
   - **Thumbnails**: URI path starts with `/t/` → Cache: Eligible, Edge TTL: 7 days
   - **Static assets**: URI path ends with `.js`/`.css`/`.svg` → Cache: Eligible

4. **Always Online** (Caching → Configuration): Enable to serve cached images if origin is down.

5. **SSL/TLS**: Set to "Full (strict)" if you have an SSL certificate on your origin,
   or "Flexible" if using HTTP only.

6. **Page Rules** (optional): Create a rule to bypass cache for `/api/*` paths.

## Other CDNs (Fastly, BunnyCDN, KeyCDN)

Similar setup — point the CDN to your origin, configure:
- Cache `/u/*` and `/t/*` with long TTL
- Bypass cache for `/api/*` and `/metrics`
- Forward `Host` and `X-Forwarded-For` headers

## Verification

```bash
# Check cache headers
curl -I https://your-domain.com/u/some-image-key 2>/dev/null | grep -i cache

# Expected:
# Cache-Control: public, max-age=31536000, immutable
# X-Cache-Status: HIT (after second request)
```

## Architecture Flow

```
User → CDN Edge → Your Server (Nginx) → PicHost API
                     ↓ (cache hit)
                  Returns cached image directly
```

- `/u/*` and `/t/*` are safe to cache aggressively (content-addressed by public_key, immutable)
- `/api/*` must bypass cache (dynamic responses, auth-required)
- Nginx proxy cache serves as origin cache — CDN is an additional layer
