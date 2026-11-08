/**
 * L'empreinte d'un envoi doit être EXACTEMENT celle du démon : c'est elle qui
 * relie un événement `prompt` à sa bulle. Vérifiée contre `node:crypto` (aux
 * bornes du bourrage SHA-256) et contre une valeur calculée côté Rust.
 */
import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { empreinteEnvoi, sha256 } from "../src/empreinte.ts";

const reference = (octets: Uint8Array): string => createHash("sha256").update(octets).digest("hex");
const hex = (octets: Uint8Array): string =>
  Array.from(octets, (octet) => octet.toString(16).padStart(2, "0")).join("");

describe("empreinte d'un envoi", () => {
  test("SHA-256 conforme aux bornes du bourrage (0, 55, 56, 63, 64, 65, 1000 octets)", () => {
    for (const taille of [0, 1, 55, 56, 63, 64, 65, 119, 120, 1000]) {
      const octets = Uint8Array.from({ length: taille }, (_, i) => (i * 31 + 7) % 256);
      expect(hex(sha256(octets))).toBe(reference(octets));
    }
  });

  test("la valeur du démon : 32 hexadécimaux de SHA-256(pane \\0 texte)", () => {
    // `empreinte_de("w1:p1", "bonjour")` côté Rust.
    expect(empreinteEnvoi("w1:p1", "bonjour")).toBe("1c262e16330173bc7fd777aeb98dc665");
  });

  test("le texte est haché en UTF-8 (accents, emoji)", () => {
    const texte = "réponds 🙂 à l'invite";
    const octets = new TextEncoder().encode(`t1\0${texte}`);
    expect(empreinteEnvoi("t1", texte)).toBe(reference(octets).slice(0, 32));
  });
});
