//! H23 — la présence d'un appareil : elle expire seule, un départ l'efface,
//! le registre est borné.

use notifications::presence::{DUREE_PRESENCE, PresenceAppareils};
use std::time::{Duration, Instant};

#[test]
fn une_presence_expire_sans_battement() {
    let presence = PresenceAppareils::nouvelle();
    let debut = Instant::now();
    presence.marque("https://push.example/a", true, debut);
    assert!(presence.est_present("https://push.example/a", debut));
    assert!(presence.est_present("https://push.example/a", debut + DUREE_PRESENCE));
    // Un appareil gelé sans avoir dit qu'il partait redevient notifiable.
    assert!(!presence.est_present(
        "https://push.example/a",
        debut + DUREE_PRESENCE + Duration::from_secs(1)
    ));
}

#[test]
fn une_presence_sur_n_importe_quel_appareil_retient_tout_le_monde() {
    let presence = PresenceAppareils::nouvelle();
    let debut = Instant::now();
    assert!(!presence.quelqu_un_est_la(debut), "personne au départ");
    // Un appareil SANS abonnement (identifiant d'appareil, pas d'endpoint).
    presence.marque("tablette-sans-abonnement", true, debut);
    assert!(presence.quelqu_un_est_la(debut));
    assert!(!presence.quelqu_un_est_la(debut + DUREE_PRESENCE + Duration::from_secs(1)));
}

#[test]
fn un_depart_efface_la_presence_tout_de_suite() {
    let presence = PresenceAppareils::nouvelle();
    let maintenant = Instant::now();
    presence.marque("https://push.example/a", true, maintenant);
    presence.marque("https://push.example/a", false, maintenant);
    assert!(!presence.est_present("https://push.example/a", maintenant));
    assert!(!presence.est_present("https://push.example/inconnu", maintenant));
}

#[test]
fn le_registre_est_borne() {
    let presence = PresenceAppareils::nouvelle();
    let debut = Instant::now();
    for n in 0..100 {
        presence.marque(
            &format!("https://push.example/{n}"),
            true,
            debut + Duration::from_millis(n),
        );
    }
    // Le plus ancien est sorti, le plus récent reste.
    let fin = debut + Duration::from_millis(100);
    assert!(!presence.est_present("https://push.example/0", fin));
    assert!(presence.est_present("https://push.example/99", fin));
}
