extern crate gtk;
extern crate gdk_pixbuf;

use gtk::gdk::ATOM_NONE;
use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, Button, ScrolledWindow};
use gtk::{Label, Window, WindowType, Image};
use gdk_pixbuf::Pixbuf;

fn main() {

    // Initialize GTK
    gtk::init().expect("Failed to initialize GTK.");

    // Create a new top-level window
    let window = Window::new(WindowType::Toplevel);

    // Set the title and size of the window
    window.set_title("Image From Amoor          Remaining Views: 10");
    window.set_default_size(800, 600);

    // Create a scrolled window to handle large images
    let scrolled_window = ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    scrolled_window.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);

    // Create an image and load it from a file
    let image_path = "pic.png";
    let image = Image::from_file(image_path);

    // Create a button to close the window
    let button = Button::with_label("Close");

    // Create a vertical box to hold the image and button
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
    vbox.pack_start(&image, false, false, 0);
    vbox.pack_start(&button, false, false, 0);

    // Connect the button's clicked signal to a callback that closes the window
    button.connect_clicked(|_| {
        gtk::main_quit();
    });

    // Set the window's content to the vertical box
    window.add(&vbox);
    window.add(&scrolled_window);
    // Show all the elements in the window
    window.show_all();

    // Run the GTK main loop
    gtk::main();
}
