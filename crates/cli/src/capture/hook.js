// The Frida hook `wxread keys` runs inside the re-signed WeChat copy
// (docs/adr/0005-keys-from-the-crypto-call.md).
//
// WeChat for Mac 4.x does not keep a database's key resident anywhere a
// memory scan can find it; the raw 32-byte key appears only for the instant
// it is handed to CommonCrypto to decrypt a page. So this hooks the three
// CommonCrypto entry points SQLCipher drives page crypto through, and for
// every AES-256 call (algorithm AES, key length 32) sends the 32-byte key up
// as hex. The effect crate proves each one against a database's first page
// before it is answered, so a wrong value is never reported.
//
// Written to work on Frida 16 and 17: 16 has Module.findExportByName, 17
// replaced it with Module.getGlobalExportByName.
const seen = {};

function findExport(name) {
  try {
    if (typeof Module.findExportByName === 'function') {
      return Module.findExportByName(null, name);
    }
  } catch (e) {}
  try {
    if (typeof Module.getGlobalExportByName === 'function') {
      return Module.getGlobalExportByName(name);
    }
  } catch (e) {}
  return null;
}

// CCCrypt(op, alg, options, key, keyLen, ...)               key=args[3] len=args[4] alg=args[1]
// CCCryptorCreate(op, alg, options, key, keyLen, ...)       key=args[3] len=args[4] alg=args[1]
// CCCryptorCreateWithMode(op, mode, alg, pad, iv, key, len) key=args[5] len=args[6] alg=args[2]
['CCCrypt', 'CCCryptorCreate', 'CCCryptorCreateWithMode'].forEach(function (name) {
  const fn = findExport(name);
  if (!fn) return;
  const withMode = name === 'CCCryptorCreateWithMode';
  const keyIdx = withMode ? 5 : 3;
  const lenIdx = withMode ? 6 : 4;
  const algIdx = withMode ? 2 : 1;
  Interceptor.attach(fn, {
    onEnter: function (args) {
      try {
        if (args[algIdx].toInt32() !== 0) return;   // kCCAlgorithmAES
        if (args[lenIdx].toInt32() !== 32) return;   // AES-256 key length
        const bytes = new Uint8Array(args[keyIdx].readByteArray(32));
        let hex = '';
        for (let i = 0; i < 32; i++) hex += bytes[i].toString(16).padStart(2, '0');
        if (!seen[hex]) { seen[hex] = 1; send({ wxkey: hex }); }
      } catch (e) {}
    },
  });
});

send({ ready: true });
