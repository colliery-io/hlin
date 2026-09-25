// What would run if a module ever loaded this: it marks the window it ran in,
// so the containment tests can tell a script that was stopped from one that
// ran quietly.
window.lured = 'the lure ran';
