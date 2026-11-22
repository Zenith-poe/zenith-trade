//! Item class to trade category, written by hand.
//!
//! The classes are the plural names the game writes after `Item Class:` in
//! a copied item. The category ids come from the `filters` list of the
//! trade site; a test checks each one against a saved copy of that list.

/// Item class, exactly as the game writes it in a copied item, to the id of
/// a trade item category. Gear, jewels, flasks, grafts and viscera only.
pub const CLASSES: &[(&str, &str)] = &[
    // One-handed weapons.
    ("Claws", "weapon.claw"),
    ("Daggers", "weapon.basedagger"),
    ("Rune Daggers", "weapon.runedagger"),
    ("One Hand Swords", "weapon.basesword"),
    ("Thrusting One Hand Swords", "weapon.rapier"),
    ("One Hand Axes", "weapon.oneaxe"),
    ("One Hand Maces", "weapon.basemace"),
    ("Sceptres", "weapon.sceptre"),
    ("Wands", "weapon.wand"),
    // Two-handed weapons.
    ("Bows", "weapon.bow"),
    ("Staves", "weapon.basestaff"),
    ("Warstaves", "weapon.warstaff"),
    ("Two Hand Swords", "weapon.twosword"),
    ("Two Hand Axes", "weapon.twoaxe"),
    ("Two Hand Maces", "weapon.twomace"),
    ("Fishing Rods", "weapon.rod"),
    // Armour and what the site files with it.
    ("Body Armours", "armour.chest"),
    ("Helmets", "armour.helmet"),
    ("Gloves", "armour.gloves"),
    ("Boots", "armour.boots"),
    ("Shields", "armour.shield"),
    ("Quivers", "armour.quiver"),
    // Accessories.
    ("Rings", "accessory.ring"),
    ("Amulets", "accessory.amulet"),
    ("Belts", "accessory.belt"),
    ("Trinkets", "accessory.trinket"),
    // Jewels. A cluster jewel is of class `Jewels` too, so the wide category.
    ("Jewels", "jewel"),
    ("Abyss Jewels", "jewel.abyss"),
    // Flasks and tinctures.
    ("Life Flasks", "flask"),
    ("Mana Flasks", "flask"),
    ("Hybrid Flasks", "flask"),
    ("Utility Flasks", "flask"),
    ("Tinctures", "tincture"),
    // What a league adds to a character. A viscera names its body part on a line of its
    // own, not in its class, so the wide category.
    ("Grafts", "graft"),
    ("Viscera", "viscera"),
];

/// The class whose trade category is under armour although none of its
/// modifiers is local: a quiver has neither defences nor damage of its own.
const QUIVERS: &str = "Quivers";

/// The trade category of an item class, when the table has it. The class
/// is compared as written: `Body Armours`, not `Body Armour`.
pub fn category(class: &str) -> Option<&'static str> {
    CLASSES
        .iter()
        .find(|(known, _)| *known == class)
        .map(|(_, id)| *id)
}

/// Whether items of this class can carry local modifiers, the ones the
/// site writes with ` (Local)`: true for the weapon and armour classes of
/// the table, quivers excepted.
pub fn is_local_class(class: &str) -> bool {
    class != QUIVERS
        && category(class).is_some_and(|id| id.starts_with("weapon.") || id.starts_with("armour."))
}
