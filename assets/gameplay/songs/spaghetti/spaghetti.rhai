// Spaghetti song script (ported from spaghetti.hxc)
// Registers song-specific note kinds in Create before notes are materialized.

fn onCreate() {
    register_notekind("sakura-joint", #{ description: "Sakura Joint Sing (Spaghetti)", suffix: "joint", scoreable: true });
    register_notekind("sakura-bf1", #{ description: "Sakura BF Pose 1 (Spaghetti)", suffix: "bf1", scoreable: true });
    register_notekind("sakura-bf2", #{ description: "Sakura BF Pose 2 (Spaghetti)", suffix: "bf2", scoreable: true });
}
