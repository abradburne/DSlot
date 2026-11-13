//! The games GBA DS (github.com/kyannon/gba-ds, GPL-3.0) gives a second screen: their map, HUD
//! or status screen on the other screen while they play. It knows a game by the four-character
//! code in its cart's header, which a romhack or a patch of a game usually keeps, so DSlot's
//! GBA DS page lists the games in `Roms/GBA` whose code is here.
//!
//! The codes are from its list of supported games, as its wiki gave it on 12 November 2026
//! (GBA DS 1.5: 51 games): https://github.com/kyannon/gba-ds/wiki/Supported-games. A game it
//! adds later is on the page once its code is added here; until then it is on the GBA page
//! alone.

/// The codes, a game a line (the Japanese Advance Wars 1+2 is on both of theirs).
const CODES: &[&str] = &[
    // Advance Wars
    "AWRE", "AWRP", "BGWJ", // Advance Wars 2: Black Hole Rising
    "AW2E", "AW2P", "BGWJ", // Castlevania: Aria of Sorrow
    "A2CE", "A2CP", "A2CJ", // Castlevania: Circle of the Moon
    "AAME", "AAMP", "AAMJ", // Castlevania: Harmony of Dissonance
    "ACHE", "ACHP", "ACHJ", // Crash Nitro Kart
    "BCNE", "BCNP", "BCNJ", // Dragon Ball Z: Buu's Fury
    "BG3E", // Dragon Ball Z: The Legacy of Goku
    "ALGE", "ALGP", // Dragon Ball Z: The Legacy of Goku II
    "ALFE", "ALFP", "ALFJ", // Eragon
    "BENE", "BENP", // F-Zero: GP Legend
    "BFZE", "BFZP", "BFZJ", // F-Zero: Maximum Velocity
    "AFZE", "AFZJ", "AFZC", // Final Fantasy Tactics Advance
    "AFXE", "AFXP", "AFXJ", // Fire Emblem
    "AE7E", "AE7X", "AE7Y", "AE7J", // Fire Emblem: The Sacred Stones
    "BE8E", "BE8P", "BE8J", // Golden Sun
    "AGSE", "AGSD", "AGSF", "AGSI", "AGSS", "AGSJ", // Golden Sun: The Lost Age
    "AGFE", "AGFD", "AGFF", "AGFI", "AGFS", "AGFJ", // GT Advance 3: Pro Concept Racing
    "A2GE", "A2GP", "A2GJ", // Harry Potter and the Prisoner of Azkaban
    "BHTE", "BHTJ", // Harvest Moon: Friends of Mineral Town
    "A4NE", "A4NP", "A4ND", "A4NJ", // Kingdom Hearts: Chain of Memories
    "B8CE", "B8CP", "B8CJ", // Lady Sia
    "ALDE", "ALDP", // The Legend of Zelda: A Link to the Past & Four Swords
    "AZLE", "AZLP", "AZLJ", // Mario Kart: Super Circuit
    "AMKE", "AMKP", "AMKJ", // Mario & Luigi: Superstar Saga
    "A88E", "A88P", "A88J", // Mega Man Battle Network
    "AREE", "AREP", "AREJ", // Metroid Fusion
    "AMTE", "AMTP", "AMTJ", // Metroid: Zero Mission
    "BMXE", "BMXP", "BMXJ", // Muppet Pinball Mayhem
    "AMWE", "AMWP", // Ninja Five-O
    "ANXE", "ANXP", // Pokémon Pinball: Ruby & Sapphire
    "BPPE", // River City Ransom EX
    "BDTE", "BDTJ", // Sonic Advance
    "ASOE", "ASOP", "ASOJ", // Sonic Advance 2
    "A2NE", "A2NP", "A2NJ", // Sonic Advance 3
    "B3SE", "B3SP", "B3SJ", // Sonic Pinball Party
    "A3VE", "A3VP", // Spider-Man: Mysterio's Menace
    "ASEE", "ASEJ", // Summon Night: Swordcraft Story
    "AB4E", "AB4J", // Summon Night: Swordcraft Story 2
    "BSKE", "BSKJ", // Super Mario Advance
    "AMAE", "AMAJ", // Super Mario Advance 2: Super Mario World
    "AA2E", "AA2P", "AA2J", // Super Mario Advance 3: Yoshi's Island
    "A3AE", "A3AP", "A3AJ", // Super Mario Advance 4: Super Mario Bros. 3
    "AX4E", "AX4P", "AX4J", // Sword of Mana
    "AVSE", "AVSY", "AVSJ", // Tactics Ogre: The Knight of Lodis
    "ATOE", "ATOJ", // Tony Hawk's Pro Skater 2
    "ATHE", "ATHF", "ATHD", "ATHJ", // Tony Hawk's Pro Skater 3
    "AT3E", "AT3F", "AT3D", // V-Rally 3
    "AVRE", "AVRP", "AVRJ", // Wario Land 4
    "AWAE", "AWAJ", // Yu-Gi-Oh! The Eternal Duelist Soul
    "AY5E", "AY5J", // Zoids: Legacy
    "AZ2E", "AZ2J",
];

/// Whether GBA DS has a second screen for the game whose cart says `code`.
pub fn supported(code: &str) -> bool {
    CODES.contains(&code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_cart_codes() {
        for code in CODES {
            assert_eq!(code.len(), 4, "{code}");
            assert!(code
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()));
        }
        let games: std::collections::HashSet<&str> = CODES.iter().map(|c| &c[..3]).collect();
        // 51 games; Game Boy Wars Advance 1+2 (BGW) is two of them on one cart.
        assert!(games.len() >= 50);
        assert!(supported("AMTE") && supported("BMXJ"));
        assert!(!supported("BPEE") && !supported(""));
    }
}
