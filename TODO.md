# Todos

## Routes 
[x] - Initialization - "/kobo/{token}/v1/initialization"

[x] - Cover images - "/kobo/{token}/{book_uuid}/{width}/{height}/{isGreyscale}/image.jpg"

[x] - Cover images with quality - "/kobo/{token}/{book_uuid}/{width}/{height}/{quality}/{is_greyscale}/image.jpg"

[x] - Library sync - "/kobo/{token}/v1/library/sync"

[x] - Book metadata - "/kobo/{token}/v1/library/{book_uuid}/metadata"

[x] - Book download - "/kobo/{token}/download/{book_id}/{book_format}"

[x] - Reading state - "/kobo/{token}/v1/library/{book_uuid}/state"

[ ] - Delete library item - DELETE req type "/v1/library/{book_uuid}"

[ ] - Create collection - "/kobo/{token}/v1/library/tags"

[ ] - Update/delete collection - "/kobo/{token}/v1/library/tags/{tag_id}"

[ ] - Add books to collection - "/kobo/{token}/v1/library/tags/{tag_id}/items"

[ ] - Remove books from collection - "/kobo/{token}/v1/library/tags/{tag_id}/items/delete"

[x] - Delete book from device - "/kobo/{token}/v1/library/{book_uuid}"

[x] - Device authentication - "/kobo/{token}/v1/auth/device"

[x] - OAuth token - "/kobo/{token}/oauth/token" 

[x] - OAuth refresh - "/kobo/{token}/oauth/refresh"

[x] - OAuth discovery - "/kobo/{token}/oauth/.well-known/openid-configuration"

[ ] Reading services stubs
  - [ ] /api/v3/content/checkforchanges
  - [ ] /api/v3/content/{...}/annotations
  - [ ] /api/UserStorage/{...}
  - [ ] /api/internal/notebooks/{...}

## Features (far)
[ ] - Hardcover reading percentage sync