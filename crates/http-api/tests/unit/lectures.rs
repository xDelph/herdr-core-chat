//! H22 — POST `/api/agents/:paneId/read` : la lecture partagée entre
//! appareils. Elle ne recule jamais, un corps invalide est un 400 nommé, et
//! la liste (H03) porte `readAt`. Lire ne vole jamais le focus du Mac, mais
//! le « vu » est partagé avec Herdr.app dans les deux sens.

use crate::unit::support::{agent, borne, envoie_post, requete_get, services};
use axum::http::StatusCode;
use domain::observation::SessionRefKind;
use http_api::router::routeur;
use test_support::fake_herdr::{HerdrFaux, Op};

#[tokio::test(flavor = "multi_thread")]
async fn la_lecture_est_partagee_et_ne_recule_jamais() {
    let faux = HerdrFaux::nouveau();
    faux.repond_snapshot(
        borne(vec![agent(
            "w1:p1",
            "claude-code",
            Some((SessionRefKind::Path, "wx.jsonl")),
        )])
        .snapshot,
    );
    let (services, _) = services(faux.clone(), Default::default(), Default::default());
    let (statut, corps) = envoie_post(
        routeur(services.clone(), vec![]),
        "/api/agents/w1:p1/read",
        br#"{"at":1000}"#.to_vec(),
    )
    .await;
    assert_eq!(statut, StatusCode::OK);
    assert_eq!(corps["paneId"], "w1:p1");
    assert_eq!(corps["readAt"], 1000);

    // Un appareil en retard ne fait pas reculer la lecture.
    let (_, corps) = envoie_post(
        routeur(services.clone(), vec![]),
        "/api/agents/w1:p1/read",
        br#"{"at":500}"#.to_vec(),
    )
    .await;
    assert_eq!(corps["readAt"], 1000);
    // Lire ne vole jamais le focus : la seule action est le « vu » partagé.
    // ⚠️ Son instant est celui du MAC (la réception) quand l'appareil est en
    // retard : `status_ms` est à l'horloge du Mac — un téléphone en retard
    // ne partageait jamais son « vu ».
    let actions = faux.actions();
    assert_eq!(actions.len(), 2, "{actions:?}");
    let maintenant = u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duree| duree.as_millis()),
    )
    .unwrap_or(u64::MAX);
    for action in &actions {
        assert_eq!(action.op, Op::MarqueVu);
        assert_eq!(action.cible, "w1:p1");
        let vu_a: u64 = action
            .detail
            .trim_start_matches("vu jusqu'à ")
            .parse()
            .unwrap_or_else(|cause| panic!("{}: {cause}", action.detail));
        assert!(
            vu_a > 1000 && vu_a <= maintenant,
            "l'horloge du Mac, pas celle de l'appareil : {vu_a}"
        );
    }

    // La liste la porte, pour tous les appareils.
    let (_, liste) = requete_get(routeur(services, vec![]), "/api/conversations").await;
    let lignes = liste["conversations"]
        .as_array()
        .unwrap_or_else(|| panic!("lignes"));
    assert_eq!(lignes[0]["readAt"], 1000);
}

#[tokio::test(flavor = "multi_thread")]
async fn un_corps_invalide_est_un_400_nomme() {
    let (services, _) = services(HerdrFaux::nouveau(), Default::default(), Default::default());
    let (statut, corps) = envoie_post(
        routeur(services, vec![]),
        "/api/agents/w1:p1/read",
        br#"{"at":"hier"}"#.to_vec(),
    )
    .await;
    assert_eq!(statut, StatusCode::BAD_REQUEST);
    assert!(
        corps["error"]
            .as_str()
            .is_some_and(|message| message.contains("`at`")),
        "{corps}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn sans_lecture_la_liste_n_invente_pas_de_read_at() {
    let faux = HerdrFaux::nouveau();
    faux.repond_snapshot(
        borne(vec![agent(
            "w1:p1",
            "claude-code",
            Some((SessionRefKind::Path, "wx.jsonl")),
        )])
        .snapshot,
    );
    let (services, _) = services(faux, Default::default(), Default::default());
    let (_, liste) = requete_get(routeur(services, vec![]), "/api/conversations").await;
    assert!(liste["conversations"][0].get("readAt").is_none(), "{liste}");
}

/// Regardé sur le Mac (`seen_at`, le « vu » de Herdr.app) : lu partout. La
/// plus récente des deux lectures l'emporte.
#[tokio::test(flavor = "multi_thread")]
async fn le_vu_de_herdr_app_compte_comme_une_lecture() {
    for (lecture, vu_mac, attendu) in [(None, Some(2000), 2000), (Some(3000), Some(2000), 3000)] {
        let faux = HerdrFaux::nouveau();
        let mut observe = agent(
            "w1:p1",
            "claude-code",
            Some((SessionRefKind::Path, "wx.jsonl")),
        );
        observe.seen_at = vu_mac;
        faux.repond_snapshot(borne(vec![observe]).snapshot);
        let (services, _) = services(faux, Default::default(), Default::default());
        if let Some(instant) = lecture {
            let _ = envoie_post(
                routeur(services.clone(), vec![]),
                "/api/agents/w1:p1/read",
                format!("{{\"at\":{instant}}}").into_bytes(),
            )
            .await;
        }
        let (_, liste) = requete_get(routeur(services, vec![]), "/api/conversations").await;
        assert_eq!(liste["conversations"][0]["readAt"], attendu, "{liste}");
    }
}

/// Un core qui refuse le « vu » n'annule pas la lecture notée ici.
#[tokio::test(flavor = "multi_thread")]
async fn un_vu_refuse_par_l_amont_garde_la_lecture() {
    let faux = HerdrFaux::nouveau();
    faux.panne_sur(
        Op::MarqueVu,
        domain::errors::PortError::Transport {
            detail: String::from("socket fermé"),
        },
    );
    let (services, _) = services(faux, Default::default(), Default::default());
    let (statut, corps) = envoie_post(
        routeur(services, vec![]),
        "/api/agents/w1:p1/read",
        br#"{"at":1000}"#.to_vec(),
    )
    .await;
    assert_eq!(statut, StatusCode::OK);
    assert_eq!(corps["readAt"], 1000);
}
