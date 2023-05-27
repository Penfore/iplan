use adw::prelude::*;
use gettextrs::gettext;
use gtk::{gdk, glib, glib::once_cell::sync::Lazy, subclass::prelude::*};
use std::cell::RefCell;

use crate::db::models::{List, Record, Task};
use crate::db::operations::{
    create_task, delete_list, new_position, read_list, read_records, read_task, read_tasks,
    update_list, update_task,
};
use crate::views::{
    project::ProjectDoneTasksWindow, project::ProjectLayout, project::TaskRow, project::TaskWindow,
    IPlanWindow,
};

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/ir/imansalmani/iplan/ui/project/project_list.ui")]
    pub struct ProjectList {
        pub list: RefCell<List>,
        pub tasks: RefCell<Vec<Task>>,
        #[template_child]
        pub header: TemplateChild<gtk::Box>,
        #[template_child]
        pub name_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub name_entry: TemplateChild<gtk::Entry>,
        #[template_child]
        pub new_task_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub options_button: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub tasks_box: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub scrolled_window: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub options_popover: TemplateChild<gtk::Popover>,
        #[template_child]
        pub show_done_tasks_button: TemplateChild<gtk::Button>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ProjectList {
        const NAME: &'static str = "ProjectList";
        type Type = super::ProjectList;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_instance_callbacks();
            klass.install_action("task.check", Some("i"), move |obj, _, value| {
                let imp = obj.imp();
                let value = value.unwrap().get().unwrap();
                let upper_row = imp.tasks_box.row_at_index(value - 1);
                let row = imp
                    .tasks_box
                    .row_at_index(value)
                    .and_downcast::<TaskRow>()
                    .unwrap();
                let task = row.task();
                if let Some(upper_row) = upper_row {
                    upper_row.grab_focus();
                }
                imp.tasks_box.remove(&row);

                let mut toast_name = task.name();
                if toast_name.chars().count() > 15 {
                    toast_name.truncate(15);
                    toast_name.push_str("...");
                }
                let toast = adw::Toast::builder()
                    .title(
                        &gettext("\"{}\" moved to the done tasks list").replace("{}", &toast_name),
                    )
                    .button_label(&gettext("Undo"))
                    .build();
                toast.connect_button_clicked(glib::clone!(
                    @weak obj, @weak task =>
                    move |_toast| {
                        task.set_property("done", false);
                        update_task(task.clone()).expect("Failed to update task");
                        let row = TaskRow::new(task);
                        obj.imp().tasks_box.append(&row);
                        row.init_widgets();
                }));
                let window = obj.root().and_downcast::<IPlanWindow>().unwrap();
                window.imp().toast_overlay.add_toast(&toast);
            });
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ProjectList {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> =
                Lazy::new(|| vec![glib::ParamSpecObject::builder::<List>("list").build()]);
            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "list" => {
                    let value = value.get::<List>().expect("value must be a List");
                    self.list.replace(value);
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "list" => self.list.borrow().to_value(),
                _ => unimplemented!(),
            }
        }
    }
    impl WidgetImpl for ProjectList {}
    impl BoxImpl for ProjectList {}
}

glib::wrapper! {
    pub struct ProjectList(ObjectSubclass<imp::ProjectList>)
        @extends gtk::Widget, gtk::Box,
        @implements gtk::Buildable;
}

#[gtk::template_callbacks]
impl ProjectList {
    pub fn new(list: List) -> Self {
        glib::Object::builder().property("list", list).build()
    }

    pub fn init_widgets(&self, project_id: i64, layout: ProjectLayout, tasks_per_page: usize) {
        // TODO: check why project_id needed
        let imp = self.imp();
        let list = self.list();

        if layout == ProjectLayout::Horizontal {
            imp.tasks_box.unparent();
            imp.scrolled_window.set_child(Some(&imp.tasks_box.get()));
            imp.scrolled_window.set_visible(true);
            let scroll_controller = gtk::EventControllerScroll::builder()
                .flags(gtk::EventControllerScrollFlags::VERTICAL)
                .build();
            scroll_controller.connect_scroll(glib::clone!(
            @weak self as obj => @default-return gtk::Inhibit(false),
                move |_controller, _dx, dy| {
                let project_lists = obj.root()
                    .and_downcast::<IPlanWindow>()
                    .unwrap()
                    .imp()
                    .project_lists
                    .get();
                let project_lists_imp = project_lists.imp();
                let viewport = project_lists_imp.scrolled_window.get().first_child()
                    .and_downcast::<gtk::Viewport>()
                    .unwrap();
                if project_lists_imp.shift_pressed.get() {
                    let adjustment = viewport.hadjustment().unwrap();
                    adjustment.set_value(
                        adjustment.value() + (adjustment.step_increment() * dy)
                    );
                    gtk::Inhibit(true)
                } else {
                    gtk::Inhibit(false)
                }
            }));
            imp.scrolled_window.add_controller(&scroll_controller);

            imp.scrolled_window.connect_edge_reached(glib::clone!(
            @weak self as obj @weak imp => @default-return (),
            move |_obj, pos| {
                if pos == gtk::PositionType::Bottom {
                    let next = imp.tasks_box.observe_children().n_items() as usize - 1;
                    let tasks = imp.tasks.borrow();
                    if next < tasks.len() {
                        let project_list_task = TaskRow::new(tasks.get(next).unwrap().clone());
                        imp.tasks_box.append(&project_list_task);
                        project_list_task.init_widgets();
                    }
                }
            }));
        }

        imp.name_button.set_label(&list.name());
        imp.name_entry.buffer().set_text(&list.name());

        let tasks = read_tasks(project_id, Some(self.list().id()), Some(false), Some(0))
            .expect("Failed to read tasks");
        imp.tasks.replace(tasks);
        let tasks = imp.tasks.borrow();
        if tasks.len() > tasks_per_page && layout == ProjectLayout::Horizontal {
            for task in tasks.split_at(tasks_per_page).0 {
                let project_list_task = TaskRow::new(task.clone());
                imp.tasks_box.append(&project_list_task);
                project_list_task.init_widgets();
            }
        } else {
            for task in tasks.clone() {
                let project_list_task = TaskRow::new(task);
                imp.tasks_box.append(&project_list_task);
                project_list_task.init_widgets();
            }
        }

        imp.tasks_box.set_sort_func(|row1, row2| {
            let row1_p = row1.property::<Task>("task").position();
            let row2_p = row2.property::<Task>("task").position();

            if row1_p < row2_p {
                gtk::Ordering::Larger
            } else {
                gtk::Ordering::Smaller
            }
        });

        imp.tasks_box.set_filter_func(glib::clone!(
        @weak imp => @default-return false,
        move |row| {
            let row = row.downcast_ref::<TaskRow>().unwrap();
            if row.task().suspended() {
                false
            } else {
                !row.imp().moving_out.get()
            }
        }));

        let list_drag_source = gtk::DragSource::builder()
            .actions(gdk::DragAction::MOVE)
            .build();
        list_drag_source.connect_prepare(glib::clone!(
        @weak self as obj => @default-return None,
        move |_drag_source, _x, _y| {
            if obj.imp().name_entry.get_visible() {
                None
            } else {
                Some(gdk::ContentProvider::for_value(&obj.to_value()))
            }}));
        list_drag_source.connect_drag_begin(|_drag_source, drag| {
            let drag_icon: gtk::DragIcon = gtk::DragIcon::for_drag(&drag).downcast().unwrap();
            let label = gtk::Label::builder().label("").build();
            drag_icon.set_child(Some(&label));
            drag.set_hotspot(0, 0);
        });
        imp.header.add_controller(&list_drag_source);

        let list_drop_target =
            gtk::DropTarget::new(ProjectList::static_type(), gdk::DragAction::MOVE);
        list_drop_target.set_preload(true);
        list_drop_target.connect_drop(glib::clone!(
            @weak self as obj => @default-return false,
            move |target, value, x, y| obj.list_drop_target_drop(target, value, x, y)));
        list_drop_target.connect_motion(glib::clone!(
            @weak self as obj => @default-return gdk::DragAction::empty(),
            move |target, x, y| obj.list_drop_target_motion(target, x, y)));
        self.add_controller(&list_drop_target);

        let task_drop_target = gtk::DropTarget::new(TaskRow::static_type(), gdk::DragAction::MOVE);
        task_drop_target.set_preload(true);
        task_drop_target.connect_drop(glib::clone!(
            @weak self as obj => @default-return false,
            move |target, value, x, y| obj.task_drop_target_drop(target, value, x, y)));
        task_drop_target.connect_motion(glib::clone!(
            @weak self as obj => @default-return gdk::DragAction::empty(),
            move |target, x, y| obj.task_drop_target_motion(target, x, y)));
        task_drop_target.connect_enter(glib::clone!(
            @weak self as obj => @default-return gdk::DragAction::empty(),
            move |target, x, y| obj.task_drop_target_enter(target, x, y)));
        task_drop_target.connect_leave(glib::clone!(
            @weak self as obj =>
            move |target| obj.task_drop_target_leave(target)));
        imp.tasks_box.add_controller(&task_drop_target);
    }

    pub fn list(&self) -> List {
        self.property("list")
    }

    pub fn select_task(&self, target_task: Task) {
        let imp = self.imp();
        let task_rows = imp.tasks_box.observe_children();
        let mut loaded = false;
        for i in 0..task_rows.n_items() - 1 {
            if let Some(project_list_task) = task_rows.item(i).and_downcast::<TaskRow>() {
                let list_task = project_list_task.task();
                if list_task.position() == target_task.position() as i32 {
                    project_list_task.grab_focus();
                    loaded = true;
                    break;
                }
            }
        }
        if !loaded {
            loop {
                let next = imp.tasks_box.observe_children().n_items() as usize - 1;
                let tasks = imp.tasks.borrow();
                if next < tasks.len() {
                    let task = tasks.get(next).unwrap().clone();
                    let task_p = task.position();
                    let project_list_task = TaskRow::new(task);
                    imp.tasks_box.append(&project_list_task);
                    project_list_task.init_widgets();
                    if task_p == target_task.position() {
                        project_list_task.grab_focus();
                        break;
                    }
                }
            }
        }
    }

    #[template_callback]
    fn handle_tasks_box_row_activated(&self, row: gtk::ListBoxRow, tasks_box: gtk::ListBox) {
        let win = self.root().and_downcast::<gtk::Window>().unwrap();
        let row = row.downcast::<TaskRow>().unwrap();
        let modal = TaskWindow::new(&win.application().unwrap(), &win, row.task());
        modal.present();
        modal.connect_close_request(glib::clone!(
            @weak row as obj => @default-return gtk::Inhibit(false),
            move |_| {
                let imp = obj.imp();
                let task = read_task(obj.task().id()).expect("Failed to read the task");
                if task.done() {
                    tasks_box.remove(&obj);
                    return gtk::Inhibit(false);
                }
                let task_name = task.name();
                imp.name_button
                    .child()
                    .unwrap()
                    .downcast::<gtk::Label>()
                    .unwrap()
                    .set_text(&task_name);
                imp.name_button.set_tooltip_text(Some(&task_name));
                imp.name_entry.buffer().set_text(&task_name);
                let records =
                read_records(task.id(), true, None, None).expect("Failed to read records");
                if !records.is_empty() {
                    imp.timer_toggle_button.set_active(true)
                } else {
                    if imp.timer_toggle_button.is_active() {
                        imp.timer_toggle_button.remove_css_class("destructive-action");
                        let handler_id = imp.timer_toggle_button_handler_id.borrow();
                        let handler_id = handler_id.as_ref().unwrap();
                        imp.timer_toggle_button.block_signal(handler_id);
                        imp.timer_toggle_button.set_active(false);
                        imp.timer_toggle_button.unblock_signal(handler_id)
                    }
                    let duration_text = if let Some(duration) = task.duration() {
                        Record::duration_display(duration)
                    } else {
                        String::new()
                    };
                    imp.timer_button_content.set_label(&duration_text);
                }
                imp.task.replace(task);
                obj.changed();
                obj.activate_action("project.update", None).expect("Failed to send project.update signal");
                gtk::Inhibit(false)
            }
        ));
    }

    #[template_callback]
    fn handle_name_button_clicked(&self, button: gtk::Button) {
        button.set_visible(false); // Entry visible param binded to this
        self.imp().name_entry.grab_focus_without_selecting();
    }

    #[template_callback]
    fn handle_name_entry_activate(&self, entry: gtk::Entry) {
        let name = entry.buffer().text();
        let list = self.list();
        let imp = self.imp();
        imp.name_button.set_label(&name);
        imp.name_button.set_visible(true);
        list.set_property("name", name);
        update_list(&list).expect("Failed to update list");
    }

    #[template_callback]
    fn handle_new_button_clicked(&self, _button: gtk::Button) {
        let list = self.list();
        let task = create_task("", list.project(), list.id(), 0).expect("Failed to create task");
        let task_ui = TaskRow::new(task);
        let imp = self.imp();
        imp.tasks_box.prepend(&task_ui);
        task_ui.init_widgets();
        let task_imp = task_ui.imp();
        task_imp.name_button.set_visible(false);
        task_imp.name_entry.grab_focus();
    }

    #[template_callback]
    fn handle_delete_button_clicked(&self, _button: gtk::Button) {
        let imp = self.imp();
        imp.options_button.popdown();
        let dialog = gtk::Builder::from_resource("/ir/imansalmani/iplan/ui/delete_dialog.ui")
            .object::<adw::MessageDialog>("dialog")
            .unwrap();
        dialog.set_transient_for(self.root().and_downcast::<gtk::Window>().as_ref());
        let dialog_heading = gettext("Delete \"{}\" list?");
        dialog.set_heading(Some(&dialog_heading.replace("{}", &self.list().name())));
        dialog.set_body(&gettext("The list and its tasks will be permanently lost."));

        dialog.connect_response(
            Some("delete"),
            glib::clone!(
            @weak self as obj =>
            move |_dialog, response| {
                if response == "delete" {
                    delete_list(obj.list().id()).expect("Failed to delete list");
                    let lists_box = obj.parent().and_downcast::<gtk::Box>().unwrap();
                    let placeholder = obj.root()
                        .and_downcast::<IPlanWindow>()
                        .unwrap()
                        .imp()
                        .project_lists
                        .imp()
                        .placeholder
                        .get();
                    lists_box.remove(&obj);
                    if lists_box.first_child().is_none() {
                        lists_box.append(&placeholder);
                    }}}),
        );
        dialog.present();
    }

    #[template_callback]
    fn handle_show_done_tasks_button_clicked(&self, _button: gtk::Button) {
        let imp = self.imp();
        imp.options_button.popdown();
        let win: IPlanWindow = self.root().and_downcast().unwrap();
        let window = ProjectDoneTasksWindow::new(win.application().unwrap(), &win, self.list());
        window.present();
    }

    fn list_drop_target_drop(
        &self,
        _target: &gtk::DropTarget,
        _value: &glib::Value,
        _x: f64,
        _y: f64,
    ) -> bool {
        // Source list moved by motion signal so it should drop on itself
        let list = self.list();
        let list_db = read_list(list.id()).expect("Failed to read list");
        if list.index() != list_db.index() {
            // TODO: add project condition
            update_list(&list).expect("Failed to update list");
        }
        true
    }

    fn list_drop_target_motion(
        &self,
        target: &gtk::DropTarget,
        _x: f64,
        _y: f64,
    ) -> gdk::DragAction {
        if let Some(source_project_list) = target.value_as::<ProjectList>() {
            let self_list = self.list();
            let source_list = source_project_list.list();
            if self_list.id() != source_list.id() {
                let parent: gtk::Box = self.parent().and_downcast().unwrap();
                let source_i = source_list.index();
                let self_i = self_list.index();
                if source_i - self_i == 1 {
                    parent.reorder_child_after(self, Some(&source_project_list));
                    source_list.set_property("index", self_i);
                    self_list.set_property("index", source_i);
                } else if source_i > self_i {
                    let lists = parent.observe_children();
                    for i in self_i..source_i {
                        let project_list =
                            lists.item(i as u32).and_downcast::<ProjectList>().unwrap();
                        project_list.list().set_property("index", i + 1);
                    }
                    if let Some(upper_list) = lists.item((self_i - 1) as u32) {
                        parent.reorder_child_after(
                            &source_project_list,
                            Some(&upper_list.downcast::<ProjectList>().unwrap()),
                        );
                    } else {
                        parent.reorder_child_after(&source_project_list, gtk::Widget::NONE);
                    }
                    source_list.set_property("index", self_i);
                } else if source_i - self_i == -1 {
                    parent.reorder_child_after(&source_project_list, Some(self));
                    source_list.set_property("index", self_i);
                    self_list.set_property("index", source_i);
                } else if source_i < self_i {
                    //
                    let lists = parent.observe_children();
                    for i in source_i + 1..self_i + 1 {
                        let project_list =
                            lists.item(i as u32).and_downcast::<ProjectList>().unwrap();
                        project_list.list().set_property("index", i - 1);
                    }
                    parent.reorder_child_after(&source_project_list, Some(self));
                    source_list.set_property("index", self_i);
                }
            }
            gdk::DragAction::MOVE
        } else {
            gdk::DragAction::empty()
        }
    }

    fn task_drop_target_drop(
        &self,
        _target: &gtk::DropTarget,
        value: &glib::Value,
        _x: f64,
        _y: f64,
    ) -> bool {
        // Source row moved by motion signal so it should drop on itself
        self.imp().tasks_box.drag_unhighlight_row();
        let row: TaskRow = value.get().unwrap();
        let task = row.task();
        let task_db = read_task(task.id()).expect("Failed to read task");
        if task_db.position() != task.position() || task_db.list() != task.list() {
            update_task(task).expect("Failed to update task");
        }
        row.grab_focus();
        true
    }

    fn task_drop_target_motion(
        &self,
        target: &gtk::DropTarget,
        _x: f64,
        y: f64,
    ) -> gdk::DragAction {
        let imp = self.imp();
        let source_row: Option<TaskRow> = target.value_as();
        let target_row = imp.tasks_box.row_at_y(y as i32);

        if source_row.is_none() || target_row.is_none() {
            return gdk::DragAction::empty();
        }
        let source_row = source_row.unwrap();
        let target_row: TaskRow = target_row.and_downcast().unwrap();

        // Move
        let source_task = source_row.task();
        let target_task = target_row.task();
        if source_task.id() != target_task.id() {
            let source_i = source_row.index();
            let target_i = target_row.index();
            let source_p = source_task.position();
            let target_p = target_task.position();
            if source_i - target_i == 1 {
                source_task.set_property("position", source_p + 1);
                target_task.set_property("position", target_p - 1);
            } else if source_i > target_i {
                for i in target_i..source_i {
                    let row: TaskRow = imp.tasks_box.row_at_index(i).and_downcast().unwrap();
                    row.task()
                        .set_property("position", row.task().position() - 1);
                }
                source_task.set_property("position", target_p)
            } else if source_i - target_i == -1 {
                source_task.set_property("position", source_p - 1);
                target_task.set_property("position", target_p + 1);
            } else if source_i < target_i {
                for i in source_i + 1..target_i + 1 {
                    let row: TaskRow = imp.tasks_box.row_at_index(i).and_downcast().unwrap();
                    row.task()
                        .set_property("position", row.task().position() + 1);
                }
                source_task.set_property("position", target_p)
            }

            // Should use invalidate_sort() insteed of changed() for refresh highlight shape
            imp.tasks_box.invalidate_sort();
        }

        // Scroll
        let scrolled_window = if imp.scrolled_window.is_visible() {
            imp.scrolled_window.get()
        } else {
            let project_lists = self
                .root()
                .and_downcast::<IPlanWindow>()
                .unwrap()
                .imp()
                .project_lists
                .get();
            project_lists.imp().scrolled_window.get()
        };
        let scrolled_window_height = scrolled_window.height();
        if imp.tasks_box.height() > scrolled_window_height {
            let adjustment = scrolled_window.vadjustment();
            let step = adjustment.step_increment() / 3.0;
            let v_pos = adjustment.value();
            if y - v_pos > (scrolled_window_height - 25) as f64 {
                adjustment.set_value(v_pos + step);
            } else if y - v_pos < 25.0 {
                adjustment.set_value(v_pos - step);
            }
        }

        gdk::DragAction::MOVE
    }

    fn task_drop_target_enter(
        &self,
        target: &gtk::DropTarget,
        _x: f64,
        _y: f64,
    ) -> gdk::DragAction {
        let row: TaskRow = target.value_as().unwrap();
        let tasks_box = self.imp().tasks_box.get();
        row.imp().moving_out.set(false);
        // Avoid from when drag start
        if row.task().list() == self.list().id() && row.imp().moving_out.get() {
            tasks_box.invalidate_filter();
        } else if row.task().list() != self.list().id() {
            let task = row.task();
            let list_id = self.list().id();
            task.set_property("list", list_id);
            task.set_property("position", new_position(list_id));
            let parent = row.parent().and_downcast::<gtk::ListBox>().unwrap();
            for i in 0..row.index() {
                let task = parent
                    .row_at_index(i)
                    .and_downcast::<TaskRow>()
                    .unwrap()
                    .task();
                task.set_property("position", task.position() - 1);
            }
            parent.remove(&row);
            tasks_box.prepend(&row);
            tasks_box.drag_highlight_row(&row);
        }
        gdk::DragAction::MOVE
    }

    fn task_drop_target_leave(&self, target: &gtk::DropTarget) {
        if let Some(row) = target.value_as::<TaskRow>() {
            row.imp().moving_out.set(true);
            self.imp().tasks_box.invalidate_filter();
        }
    }
}
