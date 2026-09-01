//! Prints one exercise per module, for eyeballing the generators.
//!
//! ```text
//! cargo run -p typing-core --example sample -- [layout] [language] [seed]
//! ```

use std::{env, fs};

use typing_core::{corpus::Corpus, exercise, goals::Module, lesson, load_layout};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let layout_name = args.next().unwrap_or_else(|| "qwerty_us".into());
    let language = args.next().unwrap_or_else(|| "en_GB".into());
    let seed: u64 = args.next().unwrap_or_else(|| "2026".into()).parse()?;

    let layout = load_layout(&layout_name).ok_or("unknown layout")?;
    let lessons = lesson::klavaro_lessons();

    let data = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/klavaro-data/corpora"
    );
    let corpus = Corpus::new(
        &language,
        &fs::read_to_string(format!("{data}/{language}.words"))?,
        &fs::read_to_string(format!("{data}/{language}.paragraphs"))?,
    );

    println!("layout {layout_name}, language {language}, seed {seed}\n");
    for module in Module::ALL {
        let lesson = if module == Module::Basic {
            Some(&lessons[9])
        } else {
            None
        };
        let exercise = exercise::generate(
            exercise::Request {
                module,
                layout: &layout,
                lesson,
                corpus: Some(&corpus),
                stop_marks: true,
            },
            seed,
        )?;
        let goals = module.goals();
        println!("== {} ==", module.slug());
        println!(
            "   goal: {:.0}% accuracy, {:.0} wpm{}\n",
            goals.accuracy,
            goals.speed,
            goals
                .fluidness
                .map(|f| format!(", {f:.0}% fluidness"))
                .unwrap_or_default()
        );
        for line in exercise.text.lines() {
            let shown: String = line.chars().take(76).collect();
            println!(
                "   {shown}{}",
                if line.chars().count() > 76 {
                    " …"
                } else {
                    ""
                }
            );
        }
        println!("   [{} characters]\n", exercise.len_chars());
    }
    Ok(())
}
