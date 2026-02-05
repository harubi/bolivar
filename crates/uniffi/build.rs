fn main() {
    uniffi::generate_scaffolding("./src/bolivar.udl")
        .expect("failed to generate UniFFI scaffolding");
}
