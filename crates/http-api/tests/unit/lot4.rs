//! H25 — POST `/api/agents/:paneId/pin` et H26 — POST
//! `/api/agents/:paneId/attachments` (lot 4) : l'épingle partagée avec
//! Herdr.app, la photo posée à côté du terminal sans rien taper.

use crate::unit::support::{envoie_post, services};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use domain::errors::PortError;
use http_api::handlers::piece_jointe::nom_assaini;
use http_api::router::routeur;
use test_support::fake_herdr::{HerdrFaux, Op};
use tower::ServiceExt;

#[tokio::test(flavor = "multi_thread")]
async fn epingler_pose_l_epingle_et_un_corps_faux_est_refuse() {
    let faux = HerdrFaux::nouveau();
    let (services, _) = services(faux.clone(), Default::default(), Default::default());
    let (statut, corps) = envoie_post(
        routeur(services.clone(), vec![]),
        "/api/agents/t7/pin",
        br#"{"pinned":true}"#.to_vec(),
    )
    .await;
    assert_eq!(statut, StatusCode::OK);
    assert_eq!(corps, serde_json::json!({"paneId": "t7", "pinned": true}));
    let geste = faux
        .actions()
        .into_iter()
        .find(|action| action.op == Op::Epingle)
        .unwrap_or_else(|| panic!("l'épingle part"));
    assert_eq!(geste.cible, "t7");
    let (statut, _) = envoie_post(
        routeur(services, vec![]),
        "/api/agents/t7/pin",
        br#"{"pin":1}"#.to_vec(),
    )
    .await;
    assert_eq!(statut, StatusCode::BAD_REQUEST);
}

#[tokio::test(flavor = "multi_thread")]
async fn epingler_un_terminal_parti_est_un_404_nomme() {
    let faux = HerdrFaux::nouveau();
    faux.panne_sur(
        Op::Epingle,
        PortError::Upstream {
            code: String::from("not_found"),
            message: String::from("no terminal t9"),
        },
    );
    let (services, _) = services(faux, Default::default(), Default::default());
    let (statut, corps) = envoie_post(
        routeur(services, vec![]),
        "/api/agents/t9/pin",
        br#"{"pinned":false}"#.to_vec(),
    )
    .await;
    assert_eq!(statut, StatusCode::NOT_FOUND);
    assert_eq!(corps["code"], "agent_gone");
}

async fn depose(
    routeur: axum::Router<()>,
    chemin: &str,
    origine: &str,
    type_contenu: &str,
    corps: Vec<u8>,
) -> (StatusCode, serde_json::Value) {
    let reponse = routeur
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(chemin)
                .header("origin", origine)
                .header("content-type", type_contenu)
                .body(Body::from(corps))
                .unwrap_or_else(|cause| panic!("requête : {cause:?}")),
        )
        .await
        .unwrap_or_else(|cause| panic!("réponse : {cause:?}"));
    let statut = reponse.status();
    let octets = axum::body::to_bytes(reponse.into_body(), 64 * 1024)
        .await
        .unwrap_or_else(|cause| panic!("corps : {cause:?}"));
    (statut, serde_json::from_slice(&octets).unwrap_or_default())
}

#[tokio::test(flavor = "multi_thread")]
async fn une_photo_est_posee_a_cote_du_terminal_et_rien_n_est_tape() {
    let faux = HerdrFaux::nouveau();
    let (services, _) = services(faux.clone(), Default::default(), Default::default());
    let (statut, corps) = depose(
        routeur(services, vec![]),
        "/api/agents/t7/attachments?name=IMG_0042.HEIC",
        "http://localhost",
        "image/heic",
        vec![1, 2, 3, 4],
    )
    .await;
    assert_eq!(statut, StatusCode::OK, "{corps}");
    assert_eq!(corps["path"], "/core/files/t7/IMG_0042.HEIC");
    assert_eq!(corps["name"], "IMG_0042.HEIC");
    assert_eq!(corps["bytes"], 4);
    let actions = faux.actions();
    assert!(actions.iter().any(|a| a.op == Op::Depot && a.cible == "t7"));
    assert!(
        actions
            .iter()
            .all(|a| !matches!(a.op, Op::EnvoiTexte | Op::EnvoiTouches | Op::Prompt)),
        "déposer n'est pas envoyer : {actions:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn la_garde_des_pieces_jointes_refuse_formulaire_origine_et_vide() {
    let faux = HerdrFaux::nouveau();
    let (services, _) = services(faux.clone(), Default::default(), Default::default());
    let r = || routeur(services.clone(), vec![]);
    let chemin = "/api/agents/t7/attachments";
    // Un formulaire HTML peut produire ces types : refusés.
    for type_contenu in [
        "multipart/form-data; boundary=x",
        "text/plain",
        "application/json",
    ] {
        let (statut, _) = depose(r(), chemin, "http://localhost", type_contenu, vec![1]).await;
        assert_eq!(statut, StatusCode::UNSUPPORTED_MEDIA_TYPE, "{type_contenu}");
    }
    let (statut, _) = depose(r(), chemin, "https://evil.example", "image/png", vec![1]).await;
    assert_eq!(statut, StatusCode::FORBIDDEN);
    let (statut, _) = depose(r(), chemin, "http://localhost", "image/png", Vec::new()).await;
    assert_eq!(statut, StatusCode::BAD_REQUEST);
    let trop = vec![0_u8; contracts::limits::MAX_ATTACHMENT_BYTES + 1];
    let (statut, _) = depose(
        r(),
        chemin,
        "http://localhost",
        "application/octet-stream",
        trop,
    )
    .await;
    assert_eq!(statut, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(
        faux.actions().iter().all(|a| a.op != Op::Depot),
        "aucun refus ne dépose quoi que ce soit"
    );
}

#[test]
fn le_nom_d_une_piece_jointe_est_assaini() {
    assert_eq!(nom_assaini(Some("../../etc/passwd"), None), "passwd");
    assert_eq!(
        nom_assaini(Some("capture écran.png"), None),
        "capture__cran.png"
    );
    assert_eq!(nom_assaini(Some(".cache"), None), "cache");
    assert_eq!(nom_assaini(None, Some("image/png")), "photo.png");
    assert_eq!(nom_assaini(Some("///"), Some("image/jpeg")), "photo.jpg");
    let long = format!("{}.png", "a".repeat(300));
    let court = nom_assaini(Some(&long), None);
    assert_eq!(court.len(), 100);
    assert!(court.ends_with(".png"));
}
