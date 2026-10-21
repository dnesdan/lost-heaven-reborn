# Vendored read-only parsers

Source: [b4rtt/mafia-workbench](https://github.com/b4rtt/mafia-workbench),
revision `9f1edbfbc20ab2ac57e66d4f8a444127e92d602e` (MIT).
The complete upstream copyright and permission notice is in [vendor/LICENSE](vendor/LICENSE).

- `vendor/fourds.mjs` adapts `src/core/fourds.js`: keeps the reader and
  `parseFourDs`; omits writers and their material-settings import; replaces
  iconv-lite decoding with Node's built-in Windows-1250 `TextDecoder`.
- `vendor/bmp.mjs` retains `parseBmp` from `src/core/bmp.js`; omits writers.
- `convert.mjs`, `png.mjs` and `cli.mjs` are original MIT project code.

Coordinate interpretation follows upstream `src/ui/transforms.js` and
`src/ui/viewport.js`: conjugate stored WXYZ for column-vector transforms,
then reflect Z from left-handed Y-up to right-handed Y-up. Converted vertex
winding is reversed to retain outward-facing normals. glTF image coordinates
start at the top, so source UVs are retained. These are format interpretations;
physical scale and original-game appearance require separate reference checks.
