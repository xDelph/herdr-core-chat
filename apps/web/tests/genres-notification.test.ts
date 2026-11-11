/**
 * Quelles notifications recevoir : le choix voyage avec l'abonnement
 * (`muted`), le Mac trie — jamais un push avalé par le service worker.
 */
import { describe, expect, test } from "bun:test";
import {
  basculeGenres,
  CHOIX_NOTIFICATIONS,
  corpsAbonnement,
  genresCoupesEnregistres,
} from "../src/genres-notification.ts";

const abonnement = { endpoint: "https://push.example/1", keys: { auth: "a", p256dh: "p" } };

describe("les genres coupés", () => {
  test("rien de retenu, ou illisible : tout est reçu", () => {
    expect(genresCoupesEnregistres(null)).toEqual([]);
    expect(genresCoupesEnregistres("{pas du json")).toEqual([]);
    expect(genresCoupesEnregistres('"done"')).toEqual([]);
    expect(genresCoupesEnregistres('["done","inconnu"]')).toEqual(["done"]);
  });

  test("« erreur » coupe aussi l'agent qui n'est plus là", () => {
    const erreur = CHOIX_NOTIFICATIONS.find((choix) => choix.cle === "erreur");
    expect(erreur?.genres).toEqual(["error", "gone"]);
    expect(basculeGenres([], erreur?.genres ?? [], false)).toEqual(["error", "gone"]);
  });

  test("rétablir un interrupteur ne touche pas aux autres", () => {
    expect(basculeGenres(["done", "error", "gone"], ["done"], true)).toEqual(["error", "gone"]);
  });

  test("le corps porte `muted` seulement s'il y a quelque chose à couper", () => {
    expect(corpsAbonnement(abonnement, [])).toEqual(abonnement);
    expect(corpsAbonnement(abonnement, ["done"])).toEqual({ ...abonnement, muted: ["done"] });
  });
});
