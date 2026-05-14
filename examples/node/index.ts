// Run:
//   nix develop --command npm install
//   nix develop --command npm run build   # produces ./pkg via wasm-pack --target nodejs
//   nix develop --command npm start

import {
    generateSvid,
    encodeHumanReadable,
    decodeHumanReadable,
    decodeHumanReadableExpecting,
    encodeBase58,
    decodeBase58,
    decodeSvid,
    extractTag,
    svidEpoch,
} from "./pkg/svid.js";

// 7-bit entity tags (0..=127). Persisted inside every ID — never renumber
// or reuse a value after deployment.
const TAG = {
    USER: 1,
    GROUP: 2,
} as const;

const userId = generateSvid(TAG.USER);
const groupId = generateSvid(TAG.GROUP);

console.log("userId  (bigint):", userId);
console.log("groupId (bigint):", groupId);

const userStr = encodeHumanReadable(userId);
const userB58 = encodeBase58(userId);
console.log("userId  (11-char):", userStr);
console.log("userId  (base58): ", userB58);

console.log("human-readable round-trip:", decodeHumanReadable(userStr) === userId);
console.log("base58 round-trip:        ", decodeBase58(userB58) === userId);

const verified = decodeHumanReadableExpecting(userStr, TAG.USER);
console.log("tag-checked decode ok:    ", verified === userId);

try {
    decodeHumanReadableExpecting(userStr, TAG.GROUP);
    console.log("tag mismatch NOT rejected (unexpected)");
} catch (e) {
    console.log("tag mismatch rejected:    ", (e as Error).message);
}

console.log("decodeSvid(userId):", decodeSvid(userId));
console.log("extractTag(userId):", extractTag(userId));
console.log("svidEpoch (unix s):", svidEpoch());
