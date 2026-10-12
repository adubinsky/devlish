# Exact keys for external JSON APIs

Unquoted Devlish names are normalized. Use quoted field names to preserve the
case of an external API's JSON keys, both in record literals and field reads:

```devlish
address equals record with "US" as "regionCode" and list of "1600 Amphitheatre Parkway, Mountain View, CA 94043" as "addressLines"
payload equals record with address as address
Post to "https://addressvalidation.googleapis.com/v1:validateAddress" with payload as validation
result equals result of body of validation
verdict equals verdict of result
complete equals "addressComplete" of verdict
shape equals record with "boolean" as "addressComplete" and "text" as "validationGranularity"
Require verdict matches shape shape
```

A quoted field name is an exact JSON string. Ordinary unquoted names keep their
existing normalization. HTTP permission scopes also preserve URL path casing.
The permissions and independent policy must authorize a request before dispatch.

String literals decode escapes once: `"\n"` contains a newline; `"\\n"`
contains a literal backslash followed by `n`. This matters when a Devlish program
writes another program as text.

Structured model responses may contain Markdown fences inside JSON string values.
The model adapter parses a complete JSON response before trying fenced/prose
wrappers, so generated documentation does not alter the surrounding JSON.


Use `length of text` for a UTF-8 byte bound (or `length of list` for an item
count). For native Google validation, configure `GOOGLE_ADDRESS_VALIDATION_API_KEY`
in the credential environment/store. The host injects `X-Goog-Api-Key` only for
POST to the exact endpoint shown above and disables redirects for that endpoint.
The credential is never a program or model value; independent permission and
policy checks still precede the effect. Generic `API_KEY` remains a separate
`X-API-Key` credential. Existing bearer-token authentication is also available.
