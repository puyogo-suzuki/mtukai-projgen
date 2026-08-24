use std::compile_error;
use unresolved_crate;

struct Fuga {
    comp_err: compile_error!("Fuga is not used")
}

impl Eq for Fuga {
    fn eq(&self, other: &Self) -> bool {
        compile_error!("Fuga is not used");
        true
    }
}

impl Fuga {
    fn new() -> Self {
        compile_error!("This is not used");
        Fuga {}
    }
}

struct Hoge {
    comp_err: compile_error!("Hoge is not used"),
    fuga : Fuga
}

trait Foo {
    fn bar(&self);
    fn baz(&self){
        compile_error!("Foo.baz is not used");
    }
    fn qnx(&self); //  Foo.qnx is not used.
}

trait Poyo {
    compile_error!("Poyo is not used");
    fn yeah(&self) { }
}

struct Pony {

}

struct Piyo {
    p : Pony
}

impl Piyo {
    fn new() -> Self {
        Self::something_new()
    }

    fn something_new() -> Self {
        unsafe { std::mem::MaybeUninit::zeroed().assume_init() }
    }

    fn notused(&self) {
        compile_error!("Piyo.notused is not used");
    }
}

impl Poyo for Piyo {
    compile_error!("Poyo is not used");
    fn yeah(&self) { }
}

impl PartialEq for Piyo {
    fn eq(&self, other: &Self) -> bool {
        true
    }
}

impl Foo for Piyo {
    fn bar(&self) {
        println!("bar");
    }

    fn baz(&self) {
        compile_error!("Foo.baz is not used");
    }
}

impl Foo for Hoge {
    compile_error!("Hoge is not used");
    fn bar(&self) {
    }

    fn baz(&self) {
    }
}

#[cfg(feature="is-lp-core")]
fn f0() {
    // DO NOT REMOVE because this function has a cfg attribute that cares "is-lp-core" feature.
}

fn f1() {
    compile_error!("f1 is not used");
}

fn main() {
    println!("Hello, world!");
    let piyo = Piyo::new();
    piyo.bar();
    let piyo2 = Piyo::new();
    let pp  = piyo == piyo2;
    println!("piyo == piyo2: {}", pp);
}
