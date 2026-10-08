//! La couche applicative : un build et une question en entrée, du JSON en
//! sortie. Elle ne connaît ni HTTP ni fenêtre, si bien que le CLI, le serveur de
//! développement et l'application de bureau partagent exactement le même calcul.

pub mod armes;
pub mod bombes;
pub mod cartes;
pub mod donnees;
pub mod effets_d_objets;
pub mod fiche;
pub mod grille;
pub mod invocations;
pub mod notes;
pub mod objets_de_classe;
pub mod portails;
pub mod reseau;
pub mod solve;
pub mod sorts_d_objets;
