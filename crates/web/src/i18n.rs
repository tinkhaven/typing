//! Interface translations.
//!
//! Where a string exists in Klavaro's own `po/*.po` catalogues, the translation
//! is taken from there — those are GPL and credited to the translators listed in
//! each catalogue, so reusing them is both allowed and the right thing to do.
//! Strings this port invents are translated here.
//!
//! The keys are an enum rather than strings, so a missing translation is a
//! compile error instead of a blank label in production. Adding a language means
//! adding one arm to [`Locale::text`].

use serde::{Deserialize, Serialize};
use typing_core::goals::Module;

/// A language the interface is available in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Locale {
    /// English.
    En,
    /// Dutch.
    Nl,
    /// French.
    Fr,
    /// German.
    De,
}

impl Locale {
    /// Every locale, in the order they should be offered.
    pub const ALL: [Locale; 4] = [Locale::En, Locale::Nl, Locale::Fr, Locale::De];

    /// The BCP-47 code, used in the `lang` attribute and for storage.
    pub fn code(self) -> &'static str {
        match self {
            Locale::En => "en",
            Locale::Nl => "nl",
            Locale::Fr => "fr",
            Locale::De => "de",
        }
    }

    /// The language's own name for itself, for the picker.
    pub fn endonym(self) -> &'static str {
        match self {
            Locale::En => "English",
            Locale::Nl => "Nederlands",
            Locale::Fr => "Français",
            Locale::De => "Deutsch",
        }
    }

    /// Parses a code, accepting regional forms such as `nl-BE`.
    pub fn from_code(code: &str) -> Option<Locale> {
        let base = code.split(['-', '_']).next().unwrap_or(code).to_lowercase();
        Locale::ALL.into_iter().find(|l| l.code() == base)
    }

    /// The corpus language to practise in by default for this interface language.
    ///
    /// Corpora are named by language code, but English's is `en_GB`.
    pub fn default_corpus(self) -> &'static str {
        match self {
            Locale::En => "en_GB",
            Locale::Nl => "nl",
            Locale::Fr => "fr",
            Locale::De => "de",
        }
    }

    /// The keyboard layout most likely to be in front of this visitor.
    pub fn likely_layout(self) -> &'static str {
        match self {
            Locale::En => "qwerty_us",
            // Belgium and the Netherlands differ here: BE is AZERTY, NL is US
            // QWERTY. Belgium is the safer default for a Dutch-language visitor
            // of a Belgian site, and it is one click to change.
            Locale::Nl => "azerty_be",
            Locale::Fr => "azerty_fr",
            Locale::De => "qwertz_de",
        }
    }

    /// Looks up a string.
    pub fn text(self, key: Msg) -> &'static str {
        let (en, nl, fr, de) = key.translations();
        match self {
            Locale::En => en,
            Locale::Nl => nl,
            Locale::Fr => fr,
            Locale::De => de,
        }
    }

    /// The name of a module.
    pub fn module_name(self, module: Module) -> &'static str {
        self.text(match module {
            Module::Basic => Msg::ModuleBasic,
            Module::Adaptability => Msg::ModuleAdaptability,
            Module::Velocity => Msg::ModuleVelocity,
            Module::Fluidness => Msg::ModuleFluidness,
        })
    }

    /// A one-line explanation of what a module is for.
    pub fn module_blurb(self, module: Module) -> &'static str {
        self.text(match module {
            Module::Basic => Msg::BlurbBasic,
            Module::Adaptability => Msg::BlurbAdaptability,
            Module::Velocity => Msg::BlurbVelocity,
            Module::Fluidness => Msg::BlurbFluidness,
        })
    }
}

/// Every translatable string in the interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Msg {
    /// The application's name.
    AppName,
    /// Tagline under the name.
    Tagline,
    /// Basic module name.
    ModuleBasic,
    /// Adaptability module name.
    ModuleAdaptability,
    /// Velocity module name.
    ModuleVelocity,
    /// Fluidness module name.
    ModuleFluidness,
    /// What Basic is for.
    BlurbBasic,
    /// What Adaptability is for.
    BlurbAdaptability,
    /// What Velocity is for.
    BlurbVelocity,
    /// What Fluidness is for.
    BlurbFluidness,
    /// Accuracy statistic label.
    Accuracy,
    /// Speed statistic label.
    Speed,
    /// Fluidness statistic label.
    Fluidness,
    /// Errors statistic label.
    Errors,
    /// Elapsed time label.
    Time,
    /// Board column heading for when a result was set.
    AchievedOn,
    /// Keyboard layout picker label.
    Keyboard,
    /// Practice language picker label.
    PracticeLanguage,
    /// Interface language picker label.
    InterfaceLanguage,
    /// Lesson picker label.
    Lesson,
    /// Prompt to begin typing.
    StartTyping,
    /// Button that fetches a new exercise.
    NewExercise,
    /// Heading of the results panel.
    Results,
    /// Told the goals were met.
    GoalMet,
    /// Told the goals were not met.
    GoalMissed,
    /// Leaderboard heading.
    Leaderboard,
    /// Nickname field label.
    Nickname,
    /// Button that publishes a result.
    Publish,
    /// Told the result is not eligible for the board.
    NotPublishable,
    /// Explains what makes a result eligible.
    PublishRules,
    /// Column heading for board position.
    Rank,
    /// Shown when a board has no entries.
    BoardEmpty,
    /// Working offline, results cannot be published.
    Offline,
    /// Link to the credits page.
    About,
    /// Correction is required in this module.
    CorrectionRequired,
}

impl Msg {
    /// The four translations, in `Locale::ALL` order.
    ///
    /// Strings marked *(Klavaro)* come from the upstream catalogues.
    fn translations(self) -> (&'static str, &'static str, &'static str, &'static str) {
        match self {
            Msg::AppName => (
                "Tinkhaven Typing",
                "Tinkhaven Typing",
                "Tinkhaven Typing",
                "Tinkhaven Typing",
            ),
            Msg::Tagline => (
                "Learn to touch type",
                "Leer blind typen",
                "Apprenez la dactylographie",
                "Lernen Sie das Zehnfingersystem",
            ),
            // Module names: Klavaro catalogues.
            Msg::ModuleBasic => ("Basic course", "Basiscursus", "Cours de base", "Grundkurs"),
            Msg::ModuleAdaptability => (
                "Adaptability",
                "Flexibiliteit",
                "Adaptabilité",
                "Anpassbarkeit",
            ),
            Msg::ModuleVelocity => ("Velocity", "Snelheid", "Vitesse", "Geschwindigkeit"),
            Msg::ModuleFluidness => (
                "Fluidness",
                "Vloeiendheid",
                "Fluidité",
                "Schreibflüssigkeit",
            ),
            Msg::BlurbBasic => (
                "Meet the keys a few at a time, without looking down.",
                "Leer de toetsen een paar tegelijk, zonder te kijken.",
                "Découvrez les touches quelques-unes à la fois, sans regarder.",
                "Lernen Sie die Tasten Stück für Stück, ohne hinzusehen.",
            ),
            Msg::BlurbAdaptability => (
                "Invented words that use the whole keyboard, so you cannot guess ahead.",
                "Verzonnen woorden over het hele toetsenbord, zodat je niet vooruit kunt gokken.",
                "Des mots inventés sur tout le clavier, pour vous empêcher de devancer.",
                "Erfundene Wörter über die ganze Tastatur, damit Sie nicht vorausraten.",
            ),
            Msg::BlurbVelocity => (
                "Real words, no punctuation. Push for speed.",
                "Echte woorden, geen interpunctie. Ga voor snelheid.",
                "De vrais mots, sans ponctuation. Jouez la vitesse.",
                "Echte Wörter, keine Zeichensetzung. Auf Geschwindigkeit.",
            ),
            Msg::BlurbFluidness => (
                "Real prose. Fix your mistakes and keep an even rhythm.",
                "Echte tekst. Verbeter je fouten en houd een gelijkmatig ritme.",
                "De la vraie prose. Corrigez vos fautes et gardez un rythme régulier.",
                "Echte Texte. Korrigieren Sie Fehler und halten Sie den Rhythmus.",
            ),
            // Statistic labels: Klavaro catalogues.
            Msg::Accuracy => ("Accuracy", "Nauwkeurigheid", "Précision", "Genauigkeit"),
            Msg::Speed => ("Speed", "Snelheid", "Vitesse", "Geschwindigkeit"),
            Msg::Fluidness => (
                "Fluidness",
                "Vloeiendheid",
                "Fluidité",
                "Schreibflüssigkeit",
            ),
            Msg::Errors => ("Errors", "Fouten", "Erreurs", "Fehler"),
            Msg::Time => ("Time", "Tijd", "Temps", "Zeit"),
            Msg::AchievedOn => ("Date", "Datum", "Date", "Datum"),
            Msg::Keyboard => ("Keyboard", "Toetsenbord", "Clavier", "Tastatur"),
            Msg::PracticeLanguage => (
                "Practice text",
                "Oefentekst",
                "Texte d'exercice",
                "Übungstext",
            ),
            Msg::InterfaceLanguage => ("Interface", "Interface", "Interface", "Oberfläche"),
            Msg::Lesson => ("Lesson", "Les", "Leçon", "Lektion"),
            Msg::StartTyping => (
                "Start typing to begin",
                "Begin te typen om te starten",
                "Commencez à taper pour démarrer",
                "Tippen Sie los, um zu beginnen",
            ),
            Msg::NewExercise => (
                "New exercise",
                "Nieuwe oefening",
                "Nouvel exercice",
                "Neue Übung",
            ),
            Msg::Results => ("Results", "Resultaten", "Résultats", "Ergebnisse"),
            Msg::GoalMet => (
                "Goal reached — on to the next module.",
                "Doel gehaald — door naar de volgende module.",
                "Objectif atteint — au module suivant.",
                "Ziel erreicht — weiter zum nächsten Modul.",
            ),
            Msg::GoalMissed => (
                "Not there yet. Try again.",
                "Nog niet. Probeer het opnieuw.",
                "Pas encore. Réessayez.",
                "Noch nicht. Versuchen Sie es erneut.",
            ),
            Msg::Leaderboard => ("Top 10", "Top 10", "Top 10", "Top 10"),
            Msg::Nickname => ("Nickname", "Bijnaam", "Pseudonyme", "Spitzname"),
            Msg::Publish => (
                "Add to the board",
                "Op het bord zetten",
                "Ajouter au classement",
                "In die Tabelle eintragen",
            ),
            Msg::NotPublishable => (
                "This run does not qualify for the board.",
                "Deze poging komt niet in aanmerking voor het bord.",
                "Cette tentative ne peut pas figurer au classement.",
                "Dieser Versuch qualifiziert sich nicht für die Tabelle.",
            ),
            Msg::PublishRules => (
                "The board takes finished Velocity and Fluidness runs that met the module's goals — Fluidness from 500 characters up.",
                "Het bord neemt afgeronde Snelheid- en Vloeiendheid-oefeningen op die de doelen haalden — Vloeiendheid vanaf 500 tekens.",
                "Le classement retient les exercices Vitesse et Fluidité terminés ayant atteint les objectifs — Fluidité à partir de 500 caractères.",
                "Die Tabelle nimmt abgeschlossene Geschwindigkeits- und Schreibflüssigkeitsübungen auf, die die Ziele erreichten — Schreibflüssigkeit ab 500 Zeichen.",
            ),
            Msg::Rank => ("#", "#", "#", "#"),
            Msg::BoardEmpty => (
                "Nobody here yet. Be the first.",
                "Nog niemand. Wees de eerste.",
                "Personne encore. Soyez le premier.",
                "Noch niemand. Seien Sie der Erste.",
            ),
            Msg::Offline => (
                "Not connected — you can practise, but results cannot be published.",
                "Niet verbonden — je kunt oefenen, maar resultaten kunnen niet worden gepubliceerd.",
                "Non connecté — vous pouvez vous exercer, mais sans publier de résultat.",
                "Nicht verbunden — Sie können üben, aber keine Ergebnisse veröffentlichen.",
            ),
            Msg::About => ("About", "Over", "À propos", "Über"),
            Msg::CorrectionRequired => (
                "Backspace over mistakes and retype them.",
                "Gebruik backspace om fouten te verbeteren.",
                "Corrigez les fautes avec la touche retour arrière.",
                "Korrigieren Sie Fehler mit der Rücktaste.",
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every message in the interface, for exhaustiveness checks.
    const EVERY_MSG: [Msg; 33] = [
        Msg::AppName,
        Msg::Tagline,
        Msg::ModuleBasic,
        Msg::ModuleAdaptability,
        Msg::ModuleVelocity,
        Msg::ModuleFluidness,
        Msg::BlurbBasic,
        Msg::BlurbAdaptability,
        Msg::BlurbVelocity,
        Msg::BlurbFluidness,
        Msg::Accuracy,
        Msg::Speed,
        Msg::Fluidness,
        Msg::Errors,
        Msg::Time,
        Msg::AchievedOn,
        Msg::Keyboard,
        Msg::PracticeLanguage,
        Msg::InterfaceLanguage,
        Msg::Lesson,
        Msg::StartTyping,
        Msg::NewExercise,
        Msg::Results,
        Msg::GoalMet,
        Msg::GoalMissed,
        Msg::Leaderboard,
        Msg::Nickname,
        Msg::Publish,
        Msg::NotPublishable,
        Msg::PublishRules,
        Msg::Rank,
        Msg::BoardEmpty,
        Msg::Offline,
    ];

    #[test]
    fn no_translation_is_missing() {
        for msg in EVERY_MSG {
            for locale in Locale::ALL {
                let text = locale.text(msg);
                assert!(
                    !text.trim().is_empty(),
                    "{msg:?} is empty in {}",
                    locale.code()
                );
            }
        }
    }

    #[test]
    fn locale_codes_round_trip() {
        for locale in Locale::ALL {
            assert_eq!(Locale::from_code(locale.code()), Some(locale));
        }
    }

    #[test]
    fn regional_codes_fall_back_to_the_base_language() {
        assert_eq!(Locale::from_code("nl-BE"), Some(Locale::Nl));
        assert_eq!(Locale::from_code("nl_NL"), Some(Locale::Nl));
        assert_eq!(Locale::from_code("en-GB"), Some(Locale::En));
        assert_eq!(Locale::from_code("de-AT"), Some(Locale::De));
        assert_eq!(Locale::from_code("pt-BR"), None, "not translated yet");
    }

    #[test]
    fn every_locale_names_every_module() {
        for locale in Locale::ALL {
            for module in Module::ALL {
                assert!(!locale.module_name(module).is_empty());
                assert!(!locale.module_blurb(module).is_empty());
            }
        }
    }

    #[test]
    fn suggested_layouts_and_corpora_all_exist() {
        for locale in Locale::ALL {
            assert!(
                typing_core::load_layout(locale.likely_layout()).is_some(),
                "{} suggests a layout that does not exist",
                locale.code()
            );
        }
    }

    #[test]
    fn the_ui_language_list_matches_the_core_crate() {
        let mine: Vec<&str> = Locale::ALL.iter().map(|l| l.code()).collect();
        assert_eq!(mine, typing_core::UI_LANGUAGES.to_vec());
    }
}
