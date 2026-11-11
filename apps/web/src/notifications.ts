import {
  CLE_GENRES_COUPES,
  corpsAbonnement,
  genresCoupesEnregistres,
  type GenreNotification,
} from "./genres-notification.ts";

/** Les genres que cet appareil a coupés (Réglages) — retenus ici. */
export const lireGenresCoupes = (): GenreNotification[] => {
  try {
    return genresCoupesEnregistres(localStorage.getItem(CLE_GENRES_COUPES));
  } catch {
    return [];
  }
};

const retenirGenresCoupes = (coupes: ReadonlyArray<GenreNotification>): void => {
  try {
    localStorage.setItem(CLE_GENRES_COUPES, JSON.stringify(coupes));
  } catch {
    // Navigation privée : le choix vaut pour cette session, et c'est tout.
  }
};

const applicationServerKey = (encoded: string): Uint8Array<ArrayBuffer> => {
  const padded = `${encoded}${"=".repeat((4 - (encoded.length % 4)) % 4)}`;
  const binary = window.atob(padded.replace(/-/g, "+").replace(/_/g, "/"));
  const key = new Uint8Array(new ArrayBuffer(binary.length));
  for (let index = 0; index < binary.length; index += 1) {
    key[index] = binary.charCodeAt(index);
  }
  return key;
};

const subscriptionRequest = async (body: unknown): Promise<void> => {
  const response = await fetch("/api/notifications/subscriptions", {
    body: JSON.stringify(body),
    headers: { "content-type": "application/json" },
    method: "POST",
  });
  if (!response.ok) {
    // ⚠️ Le corps est un `ApiErrorResponse` JSON : sa cause, pas le JSON brut
    // (« {"error":"Web Push is unavailable"} » s'affichait tel quel).
    const texte = await response.text();
    let cause = texte;
    try {
      const lu: unknown = JSON.parse(texte);
      const erreur: unknown =
        typeof lu === "object" && lu !== null ? Reflect.get(lu, "error") : undefined;
      if (typeof erreur === "string" && erreur.length > 0) cause = erreur;
    } catch {
      // Pas du JSON : le texte tel quel.
    }
    throw new Error(cause || `Notification setting rejected (${String(response.status)})`);
  }
};

export const pushSupported =
  typeof window !== "undefined" &&
  "serviceWorker" in navigator &&
  "PushManager" in window &&
  "Notification" in window;

export const currentPushSubscription = async (): Promise<PushSubscription | null> => {
  if (!pushSupported) return null;
  const registration = await navigator.serviceWorker.getRegistration();
  if (registration === undefined) return null;
  return registration.pushManager.getSubscription();
};

export const enablePushNotifications = async (): Promise<void> => {
  if (!pushSupported) throw new Error("This browser does not support push notifications.");
  const permission =
    Notification.permission === "default"
      ? await Notification.requestPermission()
      : Notification.permission;
  if (permission !== "granted") {
    throw new Error(
      permission === "denied"
        ? "Notifications are blocked in the browser settings."
        : "Notification permission was not granted.",
    );
  }

  const registration = await navigator.serviceWorker.getRegistration();
  if (registration === undefined) {
    throw new Error(
      "No service worker is registered. Install the app from the Share menu first, then try again.",
    );
  }

  const existing = await registration.pushManager.getSubscription();
  if (existing !== null) {
    await subscriptionRequest(corpsAbonnement(existing.toJSON(), lireGenresCoupes()));
    return;
  }

  const keyResponse = await fetch("/api/notifications/vapid-key", {
    cache: "no-store",
  });
  if (!keyResponse.ok) {
    throw new Error((await keyResponse.text()) || "Notification key unavailable.");
  }
  const key = (await keyResponse.text()).trim();
  if (key.length === 0) throw new Error("The notification key is empty.");

  const subscription = await registration.pushManager.subscribe({
    applicationServerKey: applicationServerKey(key),
    userVisibleOnly: true,
  });
  try {
    await subscriptionRequest(corpsAbonnement(subscription.toJSON(), lireGenresCoupes()));
  } catch (cause) {
    await subscription.unsubscribe();
    throw cause;
  }
};

export const disablePushNotifications = async (): Promise<void> => {
  const subscription = await currentPushSubscription();
  if (subscription === null) return;
  await subscriptionRequest({ unsubscribe: subscription.endpoint });
  if (!(await subscription.unsubscribe())) {
    throw new Error(
      "The subscription was removed from the server, but this browser could not unsubscribe.",
    );
  }
};

/**
 * Change les genres coupés : retenus ici, et renvoyés au démon avec
 * l'abonnement s'il existe (sinon au prochain « Turn on »). Un refus du démon
 * remonte tel quel — le choix affiché n'est pas celui du Mac tant qu'il n'a
 * pas répondu.
 */
export const appliquerGenresCoupes = async (
  coupes: ReadonlyArray<GenreNotification>,
): Promise<void> => {
  const abonnement = await currentPushSubscription();
  if (abonnement !== null) {
    await subscriptionRequest(corpsAbonnement(abonnement.toJSON(), coupes));
  }
  retenirGenresCoupes(coupes);
};
