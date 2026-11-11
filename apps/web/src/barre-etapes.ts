/**
 * La barre des étapes d'un tour (maquette « Inbox », conversation) : un
 * segment par action, vert réussi, rouge en échec, bleu pour celle en cours —
 * l'allure du tour se lit sans déplier le groupe.
 *
 * ⚠️ **Au plus `MAX_SEGMENTS` segments.** Un tour de 200 actions ferait des
 * traits d'un pixel ; au-delà, les actions se regroupent par paquets égaux, et
 * un paquet qui contient un échec reste rouge — un échec ne se dilue jamais.
 */
export type Segment = "ok" | "echec" | "cours";

export const MAX_SEGMENTS = 32;

export const segmentsDe = (
  echecs: ReadonlyArray<boolean>,
  enCours: boolean,
): ReadonlyArray<Segment> => {
  const total = echecs.length;
  if (total === 0) return [];
  const paquets = Math.min(total, MAX_SEGMENTS);
  const segments: Segment[] = [];
  for (let paquet = 0; paquet < paquets; paquet += 1) {
    const debut = Math.floor((paquet * total) / paquets);
    const fin = Math.floor(((paquet + 1) * total) / paquets);
    const dedans = echecs.slice(debut, fin);
    const dernier = paquet === paquets - 1;
    segments.push(dedans.includes(true) ? "echec" : enCours && dernier ? "cours" : "ok");
  }
  return segments;
};
