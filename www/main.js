// Loads the Rust/WebAssembly build; everything else happens in src/web.rs.
import init from "./pkg/ticket_tracker.js";

init().catch((err) => {
  document.getElementById("status").textContent = "Could not start: " + err;
});
