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
