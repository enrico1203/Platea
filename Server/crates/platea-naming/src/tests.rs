use super::*;

fn movie(p: &str) -> (String, Option<u16>) {
    let m = parse_movie(p);
    (m.title, m.year)
}

#[test]
fn movies_table() {
    let cases: &[(&str, &str, Option<u16>)] = &[
        ("The Matrix (1999)/The Matrix (1999).mkv", "The Matrix", Some(1999)),
        ("The Matrix (1999) [tmdbid-603]/The Matrix (1999).mkv", "The Matrix", Some(1999)),
        ("The.Matrix.1999.1080p.BluRay.x264-GROUP.mkv", "The Matrix", Some(1999)),
        ("Film/Inception.2010.2160p.UHD.BluRay.REMUX.HDR.HEVC.mkv", "Inception", Some(2010)),
        ("2001 A Space Odyssey (1968).mkv", "2001 A Space Odyssey", Some(1968)),
        ("1917 (2019)/1917 (2019).mp4", "1917", Some(2019)),
        ("Blade Runner 2049 (2017).mkv", "Blade Runner 2049", Some(2017)),
        ("Dune Parte Due (2024)/Dune Parte Due (2024) - 4K.mkv", "Dune Parte Due", Some(2024)),
        ("Amelie_2001_DVDRip.avi", "Amelie", Some(2001)),
        ("La vita è bella (1997).mkv", "La vita è bella", Some(1997)),
        ("Movies/Alien.mkv", "Alien", None),
        ("Interstellar.2014.ITA.ENG.1080p.mkv", "Interstellar", Some(2014)),
        ("Star.Wars.Episode.IV.A.New.Hope.1977.720p.mkv", "Star Wars Episode IV A New Hope", Some(1977)),
        ("Prova HEVC (2022)/Prova HEVC (2022).mkv", "Prova HEVC", Some(2022)),
        ("Remux Story (2021).mkv", "Remux Story", Some(2021)),
        ("Mission Impossible - Dead Reckoning (2023) [imdbid-tt9603212]/movie.mkv", "Mission Impossible - Dead Reckoning", Some(2023)),
    ];
    for (path, title, year) in cases {
        assert_eq!(movie(path), (title.to_string(), *year), "percorso: {path}");
    }
}

#[test]
fn movie_ids_and_versions() {
    let m = parse_movie("The Matrix (1999) [tmdbid-603]/The Matrix (1999) - 4K HDR.mkv");
    assert_eq!(m.ids.tmdb, Some(603));
    assert_eq!(m.version.as_deref(), Some("4K HDR"));
    let m2 = parse_movie("The Matrix (1999) [tmdbid-603]/The Matrix (1999) - 1080p.mkv");
    assert_eq!(m.group_key, m2.group_key);
    let m = parse_movie("Heat (1995) {imdb-tt0113277}/Heat.mkv");
    assert_eq!(m.ids.imdb.as_deref(), Some("tt0113277"));
    let m = parse_movie("Heat.1995.1080p.mkv");
    assert_eq!(m.version.as_deref(), Some("1080p"));
}

fn ep(p: &str) -> (String, Option<u32>, Vec<u32>) {
    let e = parse_episode(p).unwrap_or_else(|| panic!("non riconosciuto: {p}"));
    (e.series_title, e.season, e.episodes)
}

#[test]
fn episodes_table() {
    let cases: &[(&str, &str, Option<u32>, &[u32])] = &[
        ("Breaking Bad/Season 01/Breaking Bad S01E02.mkv", "Breaking Bad", Some(1), &[2]),
        ("Breaking Bad (2008)/Season 1/S01E03 - Title.mkv", "Breaking Bad", Some(1), &[3]),
        ("The Office (US)/Season 02/The.Office.US.S02E01.720p.mkv", "The Office (US)", Some(2), &[1]),
        ("Lost/Stagione 3/Lost 3x05.avi", "Lost", Some(3), &[5]),
        ("Friends/Season 01/Friends S01E01-E02.mkv", "Friends", Some(1), &[1, 2]),
        ("Friends/Season 01/Friends S01E16E17.mkv", "Friends", Some(1), &[16, 17]),
        ("Friends/Season 01/Friends.S01E01-03.mkv", "Friends", Some(1), &[1, 2, 3]),
        ("Dark/Specials/Dark S00E01.mkv", "Dark", Some(0), &[1]),
        ("Dark/Specials/Making of 01.mkv", "Dark", Some(0), &[1]),
        ("Severance.S02E10.1080p.WEB.h264.mkv", "Severance", Some(2), &[10]),
        ("Shogun (2024)/S01/Shogun.2024.S01E04.2160p.mkv", "Shogun", Some(1), &[4]),
        ("Doctor Who/Season 1/Doctor Who 1x01-1x02.mkv", "Doctor Who", Some(1), &[1, 2]),
        ("Fargo/Season 4/Episode 3.mkv", "Fargo", Some(4), &[3]),
    ];
    for (path, title, season, eps) in cases {
        assert_eq!(ep(path), (title.to_string(), *season, eps.to_vec()), "percorso: {path}");
    }
}

#[test]
fn episode_by_date_and_absolute() {
    let e = parse_episode("The Daily Show/The Daily Show 2024.03.12.mkv").unwrap();
    assert_eq!(e.air_date.as_deref(), Some("2024-03-12"));
    assert_eq!(e.series_title, "The Daily Show");
    let e = parse_episode("One Piece/[SubGroup] One Piece - 1071 [1080p].mkv").unwrap();
    assert_eq!(e.absolute, Some(1071));
    assert_eq!(e.season, None);
    let e = parse_episode("Naruto [tvdbid-78857]/Naruto - 012 [720p].mkv").unwrap();
    assert_eq!(e.absolute, Some(12));
    assert_eq!(e.series_ids.tvdb, Some(78857));
    assert_eq!(e.series_title, "Naruto");
}

#[test]
fn extras_and_ignored() {
    assert!(is_extra("Matrix (1999)/Extras/Making of.mkv"));
    assert!(is_extra("Matrix (1999)/Matrix-trailer.mp4"));
    assert!(is_extra("Matrix (1999)/Featurettes/x.mkv"));
    assert!(!is_extra("Matrix (1999)/Matrix (1999).mkv"));
    assert!(is_ignored_entry(".DS_Store"));
    assert!(is_ignored_entry("@eaDir"));
    assert!(is_video_file("a/b/c.MKV"));
    assert!(!is_video_file("a/b/c.nfo"));
    assert!(!is_video_file(".mkv"));
}

#[test]
fn titles() {
    assert_eq!(sort_title("The Matrix"), "matrix");
    assert_eq!(sort_title("Il Padrino"), "padrino");
    assert_eq!(sort_title("L'odio"), "odio");
    assert_eq!(normalize_title("Amélie: Le Fabuleux"), "amelie le fabuleux");
    assert!(title_similarity("The Matrix", "Matrix, The") > 0.8);
    assert!(title_similarity("Dune", "Dune Part Two") < 0.7);
    assert_eq!(title_similarity("Schindler's List", "Schindlers List"), 1.0);
}

#[test]
fn subtitles() {
    assert_eq!(subtitle_language("Film (2020).it.forced.srt"), (Some("it".into()), true, false));
    assert_eq!(subtitle_language("Film (2020).eng.srt"), (Some("eng".into()), false, false));
    assert!(is_subtitle_file("x.srt"));
}

#[test]
fn extras_kind_and_owner() {
    let e = extra_info("Matrix (1999)/Featurettes/Making of.mkv").unwrap();
    assert_eq!((e.kind, e.title.as_str(), e.owner_dir.as_str()), ("featurette", "Making of", "Matrix (1999)"));
    let e = extra_info("Matrix (1999)/Matrix-trailer.mp4").unwrap();
    assert_eq!((e.kind, e.owner_dir.as_str()), ("trailer", "Matrix (1999)"));
    let e = extra_info("Film/Matrix (1999)/Behind The Scenes/Bullet_time.mkv").unwrap();
    assert_eq!((e.kind, e.title.as_str(), e.owner_dir.as_str()), ("behindthescenes", "Bullet time", "Film/Matrix (1999)"));
    let e = extra_info("Breaking Bad (2008)/Extras/Intervista.mkv").unwrap();
    assert_eq!((e.kind, e.owner_dir.as_str()), ("extra", "Breaking Bad (2008)"));
    assert!(extra_info("Matrix (1999)/Sample/sample.mkv").is_none());
    assert!(extra_info("Matrix (1999)/Matrix (1999).mkv").is_none());
}

/// Trattino basso come separatore: per `\b` della crate regex `_` è una lettera, quindi
/// `_S01E01` e `_Episodio_01` non venivano riconosciuti e il file era scartato.
#[test]
fn underscore_separators() {
    let cases: &[(&str, Option<u32>, &[u32])] = &[
        ("Serie/Season 01/La_storia_degli_S01E01_Episodio.mkv", Some(1), &[1]),
        ("Serie/Stagione 1/La_storia_degli_S01E05_Titolo_lungo.mkv", Some(1), &[5]),
        ("Serie/La_storia_degli_S02E03E04_x265.mkv", Some(2), &[3, 4]),
        ("Serie/La_storia_degli_1x07_Finale.mkv", Some(1), &[7]),
        ("Serie/Season 1/La_storia_Episodio_03.mkv", Some(1), &[3]),
        ("Serie/Season 1/La_storia_Ep_04_1080p.mkv", Some(1), &[4]),
        ("Serie/Season 1/La_storia_degli_883_-_S01E06_-_TBA.mkv", Some(1), &[6]),
        ("Serie/Season 1/La storia degli 883 - S01E06 - TBA.mkv", Some(1), &[6]),
        ("Cherry Season (2014)/Cherry_Season_–_La_stagione_del_cuore_1×52_–_Streaming_Ita.mkv", Some(1), &[52]),
        ("Hardcore Pawn (2010)/Season 1/Season 1E02.mkv", Some(1), &[2]),
        ("80nostalgia/Stagione 2/SS02E05.mkv", Some(2), &[5]),
        ("80nostalgia/Specials/SS0E01.mkv", Some(0), &[1]),
        ("H2O (2006)/Season 1/[S1.Ep10] H2O - La verità della videocamera.mkv", Some(1), &[10]),
        ("Adventure Time (2010)/S01/7.mkv", Some(1), &[7]),
        ("Monogatari/Season 4/12.mp4", Some(4), &[12]),
    ];
    for (path, season, eps) in cases {
        let e = parse_episode(path).unwrap_or_else(|| panic!("non riconosciuto: {path}"));
        assert_eq!((e.season, e.episodes.as_slice()), (*season, *eps), "percorso: {path}");
    }
    // Senza cartella della serie il titolo viene dal nome, con gli spazi al posto dei trattini bassi.
    let e = parse_episode("La_storia_degli_883_S01E02_Titolo.mkv").unwrap();
    assert_eq!((e.series_title.as_str(), e.season, e.episodes.as_slice()), ("La storia degli 883", Some(1), &[2][..]));
    // Data con trattini bassi.
    let e = parse_episode("Show/Show_2024_03_15_Puntata.mkv").unwrap();
    assert_eq!(e.air_date.as_deref(), Some("2024-03-15"));
}

/// Forme di numerazione trovate nelle librerie vere che prima finivano tra i "non riconosciuti".
#[test]
fn loose_numbering() {
    // (percorso, stagione, episodi, mezzo episodio)
    type Case<'a> = (&'a str, Option<u32>, &'a [u32], Option<char>);
    let cases: &[Case] = &[
        ("Doraemon 2005/001-002.Doraemon.2005.@CartoonAnimeITA.mkv", None, &[1, 2], None),
        ("Chi's Sweet Home (2008)/05-08.CHI.Casa.Dolce.Casa.mkv", None, &[5, 6, 7, 8], None),
        ("Sailor Moon/Sailor Moon (128) [DVDRip AC3 ITA JAP Sub ITA].mkv", None, &[128], None),
        ("Bosko - Cortometraggi/Bosko (7).avi", None, &[7], None),
        ("Peperino Cortometraggi/15)Capitan Paperino - Sea Scouts (1939).mkv", None, &[15], None),
        ("Cristoforo Colombo (1968)/3.mkv", None, &[3], None),
        ("La vita privata delle piante (1995)/La vita privata delle piante - 4 - Piante in marcia.mkv", None, &[4], None),
        ("A che punto è la notte (0)/A che Punto é la Notte 1x1.mkv", Some(1), &[1], None),
        ("Claymore (2007)/Claymore01.mkv", None, &[1], None),
        ("Saiunkoku (2006)/Season 1/-UMS-SaiunkokuMonogatari02[DVDrip].avi.mkv", Some(1), &[2], None),
        ("Le Ragazze (2018)/Le Ragazze_S2020_Puntata 3.mkv", Some(2020), &[3], None),
        ("Anche i ricchi piangono (1979)/Anche i R/Anche i ricchi piangono 51ª [101+102] puntata (Antenna Sud).mp4", None, &[51], None),
        ("Curious George (2006)/Stagione 1/1x02a.CC.George.(Alex69Picci).mkv", Some(1), &[2], Some('A')),
        ("Cyborg Kuro-Chan (1999)/01b.Kurochan.@CartoonAnimeITA.mkv", None, &[1], Some('B')),
        ("Tom E Jerry Kids/Tom E Jerry Kids - Ita - 02C Amico Mio.mkv", None, &[2], Some('C')),
        ("Serie/Season 1/Serie S01E04b.mkv", Some(1), &[4], Some('B')),
        // `-720p` non è un intervallo di episodi.
        ("Serie/Serie S01E01-720p.mkv", Some(1), &[1], None),
        ("Serie/Serie S01E01-E03.mkv", Some(1), &[1, 2, 3], None),
        ("Luna Park (1979)/03 Luna park - Puntata del 07-04-1979.mp4", None, &[3], None),
        ("La vita sulla Terra/02 - 600 milioni di anni fa.divx", None, &[2], None),
        ("Peperino Cortometraggi/45) DONALD DUCK The Spirit of 43 (HD 1080).mkv", None, &[45], None),
        ("Paradise Kiss (2005)/[Tsunami] - Paradise Kiss - 04 - George (RAW 720p H265 10 Bit FLAC ITA-JAP).mkv", None, &[4], None),
        ("Bates Motel (2013)/Bates Motel - S05 - 09 - Orario delle visite.mkv", Some(5), &[9], None),
    ];
    for (path, season, eps, seg) in cases {
        let e = parse_episode(path).unwrap_or_else(|| panic!("non riconosciuto: {path}"));
        assert_eq!((e.season, e.episodes.as_slice(), e.segment), (*season, *eps, *seg), "percorso: {path}");
    }
    // Una parola che inizia con una lettera a–d dopo il numero non è un mezzo episodio.
    assert_eq!(parse_episode("Serie/Season 1/Serie 1x05Dark.mkv").and_then(|e| e.segment), None);
    // Cortometraggi con il solo anno: non sono episodi.
    assert!(parse_episode("Walt Disney/Alice.Gets.in.Dutch.1924.480p.DVD.DD.2.0.ENG.Sub.ENG.x264-HANDJOB.mkv").is_none());
}

#[test]
fn titles_without_year_or_with_odd_year() {
    assert_eq!(parse_title_year("1975 - L’anno che ha stravolto il mondo () WEBDL-1080p []"), ("1975 - L’anno che ha stravolto il mondo".into(), None));
    assert_eq!(parse_title_year("Quello Che Gli Altri Non Vedono () Unknown [tt27942025]"), ("Quello Che Gli Altri Non Vedono".into(), None));
    assert_eq!(parse_title_year("Stanze da Sogno (0)"), ("Stanze da Sogno".into(), None));
    assert_eq!(parse_title_year("Il.Castello.Nel.Cielo1986.ITA.AC3.Sub.1080p.BluRay.x264-HRS"), ("Il Castello Nel Cielo".into(), Some(1986)));
    assert_eq!(parse_title_year("[REC] (2007) Bluray-720p"), ("REC".into(), Some(2007)));
    assert_eq!(parse_title_year("Borgman 2030"), ("Borgman 2030".into(), None));
    assert_eq!(parse_title_year("Blade Runner 2049 (2017)"), ("Blade Runner 2049".into(), Some(2017)));
    assert_eq!(parse_title_year("Space 1999"), ("Space".into(), Some(1999)));
    // Un titolo che finisce con un anno senza etichette dopo resta com'è.
    assert_eq!(parse_title_year("Apollo1969"), ("Apollo1969".into(), None));
    let m = parse_movie("Film_2019/Film_2019_1080p_tt1234567.mkv");
    assert_eq!((m.version.as_deref(), m.ids.imdb.as_deref()), (Some("1080p"), Some("tt1234567")));
}

#[test]
fn extras_and_subtitles() {
    assert!(is_extra("Alex Cross (2024)/Alex Cross trailer.mkv"));
    assert!(is_extra("Film (2020)/Film_trailer.mkv"));
    assert!(is_extra("Josee (2020)/Bonus/NCED - 1080p.mkv"));
    assert_eq!(extra_info("Josee (2020)/Bonus/NCED.mkv").map(|x| (x.kind, x.owner_dir)), Some(("extra", "Josee (2020)".into())));
    assert!(!is_extra("The Big Short (2015)/The_Big_Short.mkv"));
    assert!(!is_extra("Power On (2021)/Stagione 00/S00E01 Series Trailer.mp4"));
    let e = parse_episode("La sberla (1978)/Puntata del 05.10.1978.mp4").unwrap();
    assert_eq!(e.air_date.as_deref(), Some("1978-10-05"));
    let e = parse_episode("Wrestling/WWE.Raw.02.05.2022.720p.HDTV.AC3.iTALiAN.x264-SpyRo.mp4").unwrap();
    assert_eq!(e.air_date.as_deref(), Some("2022-05-02"));
    let v = "Terrace House S02E046_Addio";
    assert_eq!(subtitle_tags(v, "Terrace House S02E046_Addio.Italiano.srt"), (Some("ita".into()), false, false));
    assert_eq!(subtitle_tags(v, "Terrace House S02E046_Addio.Inglese.forced.srt"), (Some("eng".into()), true, false));
    assert_eq!(subtitle_tags("Film (2020)", "Film (2020).it.srt"), (Some("it".into()), false, false));
    assert_eq!(subtitle_tags("Film (2020)", "Film (2020)_ita.srt"), (Some("ita".into()), false, false));
    assert_eq!(subtitle_tags("Film (2020)", "Film (2020).heb.srt"), (Some("heb".into()), false, false));
    // Senza nulla dopo il nome del video: lingua sconosciuta, anche se il titolo contiene "Le" o "The".
    assert_eq!(subtitle_tags("Le.Monde.The.End", "Le.Monde.The.End.srt"), (None, false, false));
    assert_eq!(subtitle_tags("12 Monkeys - S01E05", "12 Monkeys - S01E05.XviD-Pir8.en.srt").0.as_deref(), Some("en"));
}
