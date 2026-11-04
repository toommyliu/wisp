use super::*;
use core::prelude::v1::test;
use lucent_host::Host;
use lucent_host::accounts::{AccountEdit, Password};
use lucent_host::testing::headless_host;
use std::cell::RefCell;
use std::pin::pin;
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Poll, Wake, Waker};
use std::time::{Duration, Instant};

type Launches = Rc<RefCell<Vec<String>>>;

fn setup<'a>(
    host: &Host,
    cx: &'a mut TestAppContext,
) -> (Entity<AccountManager>, &'a mut VisualTestContext, Launches) {
    cx.executor().allow_parking();
    cx.update(lucent_ui::text_field::init);
    let launches: Launches = Default::default();
    let recorded = launches.clone();
    let accounts = host.accounts().clone();
    let (manager, cx) = cx.add_window_view(move |_, cx| {
        let manager = AccountManager::new(accounts, cx);
        cx.subscribe_self(move |_, Launch(username): &Launch, _| {
            recorded.borrow_mut().push(username.clone())
        })
        .detach();
        manager
    });
    cx.update(|window, _| window.activate_window());
    (manager, cx, launches)
}

fn settle(cx: &mut VisualTestContext, mut done: impl FnMut(&mut VisualTestContext) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        if done(cx) {
            return;
        }
        assert!(Instant::now() < deadline, "the window never settled");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn drawn(cx: &mut VisualTestContext, selector: &'static str) -> bool {
    cx.debug_bounds(selector).is_some()
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is not drawn"));
    cx.simulate_click(bounds.center(), Default::default());
    cx.run_until_parked();
}

fn type_into(cx: &mut VisualTestContext, field: &'static str, text: &str) {
    click(cx, field);
    cx.simulate_keystrokes("cmd-a backspace");
    cx.simulate_input(text);
    cx.run_until_parked();
}

struct Unpark(std::thread::Thread);
impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

fn wait<T, E: std::fmt::Debug>(reply: lucent_host::reply::Reply<T, E>) -> T {
    let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut reply = pin!(reply);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Poll::Ready(result) = reply
            .as_mut()
            .poll(&mut std::task::Context::from_waker(&waker))
        {
            return result.expect("the host answered with an error");
        }
        assert!(Instant::now() < deadline, "the host never answered");
        std::thread::park_timeout(Duration::from_millis(50));
    }
}

fn stored(host: &Host) -> Vec<(String, String, String)> {
    wait(host.accounts().snapshot())
        .accounts()
        .iter()
        .map(|account| {
            (
                account.label().to_owned(),
                account.username().to_owned(),
                account.password().reveal_for_editor().to_owned(),
            )
        })
        .collect()
}

fn create(host: &Host, label: Option<&str>, username: &str) {
    wait(host.accounts().edit(AccountEdit::CreateAccount {
        label: label.map(str::to_owned),
        username: username.into(),
        password: Password::new("hunter2"),
    }));
}

fn row(username: &str) -> &'static str {
    format!("account-{username}").leak()
}

fn actions(username: &str) -> &'static str {
    format!("actions-{username}").leak()
}

fn field_value(
    manager: &Entity<AccountManager>,
    cx: &mut VisualTestContext,
    field: Field,
) -> String {
    manager.read_with(cx, |manager, cx| {
        manager
            .dialog
            .as_ref()
            .expect("the account dialog is open")
            .read(cx)
            .field(field)
            .read(cx)
            .value()
            .to_owned()
    })
}

#[gpui::test]
fn the_list_follows_edits_from_anywhere(cx: &mut TestAppContext) {
    let host = headless_host();
    create(&host, Some("Main"), "alice");
    let (_, cx, _) = setup(&host, cx);
    settle(cx, |cx| drawn(cx, row("alice")));
    assert!(!drawn(cx, "accounts-empty"));

    create(&host, None, "bob");
    settle(cx, |cx| drawn(cx, row("bob")));

    wait(host.accounts().edit(AccountEdit::DeleteAccounts {
        usernames: vec!["alice".into(), "bob".into()],
    }));
    settle(cx, |cx| !drawn(cx, row("alice")) && !drawn(cx, row("bob")));
    assert!(drawn(cx, "accounts-empty"));
}

#[gpui::test]
fn adding_an_account_saves_it_and_closes_the_dialog(cx: &mut TestAppContext) {
    let host = headless_host();
    let (manager, cx, _) = setup(&host, cx);
    settle(cx, |cx| drawn(cx, "accounts-empty"));

    click(cx, "add-account");
    assert!(drawn(cx, "account-dialog"));
    cx.simulate_input("alice");
    cx.simulate_keystrokes("tab");
    cx.simulate_input("hunter2");
    cx.simulate_keystrokes("tab tab");
    cx.simulate_input("Main");
    cx.simulate_keystrokes("enter");

    settle(cx, |cx| !drawn(cx, "account-dialog"));
    assert_eq!(
        stored(&host),
        [("Main".into(), "alice".into(), "hunter2".into())]
    );
    settle(cx, |cx| drawn(cx, row("alice")));
    manager.read_with(cx, |manager, _| assert!(manager.dialog.is_none()));
}

#[gpui::test]
fn add_another_saves_and_starts_a_fresh_form(cx: &mut TestAppContext) {
    let host = headless_host();
    let (manager, cx, _) = setup(&host, cx);
    settle(cx, |cx| drawn(cx, "accounts-empty"));

    click(cx, "add-account");
    type_into(cx, "account-username", "alice");
    type_into(cx, "account-password", "hunter2");
    click(cx, "Add another");

    settle(cx, |_| stored(&host).len() == 1);
    settle(cx, |cx| {
        drawn(cx, "account-dialog") && field_value(&manager, cx, Field::Username).is_empty()
    });
    assert_eq!(field_value(&manager, cx, Field::Password), "");
    assert_eq!(stored(&host)[0].0, "alice");
}

#[gpui::test]
fn blank_fields_are_named_and_nothing_is_saved(cx: &mut TestAppContext) {
    let host = headless_host();
    let (_, cx, _) = setup(&host, cx);
    settle(cx, |cx| drawn(cx, "accounts-empty"));

    click(cx, "add-account");
    cx.simulate_input("   ");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(drawn(cx, "username-error"));
    assert!(drawn(cx, "password-error"));
    assert!(drawn(cx, "account-dialog"));

    type_into(cx, "account-username", "alice");
    assert!(!drawn(cx, "username-error"));
    assert!(drawn(cx, "password-error"));
    assert_eq!(stored(&host), []);
}

#[gpui::test]
fn a_taken_username_keeps_the_dialog_open_with_the_reason(cx: &mut TestAppContext) {
    let host = headless_host();
    create(&host, None, "alice");
    let (_, cx, _) = setup(&host, cx);
    settle(cx, |cx| drawn(cx, row("alice")));

    click(cx, "add-account");
    cx.simulate_input("ALICE");
    cx.simulate_keystrokes("tab");
    cx.simulate_input("other");
    cx.simulate_keystrokes("enter");

    settle(cx, |cx| drawn(cx, "account-error"));
    assert!(drawn(cx, "account-dialog"));
    assert_eq!(stored(&host).len(), 1);
}

#[gpui::test]
fn editing_prefills_the_account_and_saves_a_rename(cx: &mut TestAppContext) {
    let host = headless_host();
    create(&host, None, "alice");
    let (manager, cx, _) = setup(&host, cx);
    settle(cx, |cx| drawn(cx, row("alice")));

    click(cx, actions("alice"));
    click(cx, "Edit");
    assert_eq!(field_value(&manager, cx, Field::Username), "alice");
    assert_eq!(field_value(&manager, cx, Field::Password), "hunter2");
    assert_eq!(field_value(&manager, cx, Field::Label), "");

    type_into(cx, "account-username", "alicia");
    type_into(cx, "account-label", "Main");
    click(cx, "Save changes");

    settle(cx, |cx| !drawn(cx, "account-dialog"));
    assert_eq!(
        stored(&host),
        [("Main".into(), "alicia".into(), "hunter2".into())]
    );
    settle(cx, |cx| {
        drawn(cx, row("alicia")) && !drawn(cx, row("alice"))
    });
}

#[gpui::test]
fn clearing_the_label_falls_back_to_the_username(cx: &mut TestAppContext) {
    let host = headless_host();
    create(&host, Some("Main"), "alice");
    let (manager, cx, _) = setup(&host, cx);
    settle(cx, |cx| drawn(cx, row("alice")));

    click(cx, actions("alice"));
    click(cx, "Edit");
    assert_eq!(field_value(&manager, cx, Field::Label), "Main");
    type_into(cx, "account-label", "");
    click(cx, "Save changes");

    settle(cx, |cx| !drawn(cx, "account-dialog"));
    assert_eq!(stored(&host)[0].0, "alice");
}

#[gpui::test]
fn the_password_shows_only_on_request(cx: &mut TestAppContext) {
    let host = headless_host();
    create(&host, None, "alice");
    let (manager, cx, _) = setup(&host, cx);
    settle(cx, |cx| drawn(cx, row("alice")));

    click(cx, actions("alice"));
    click(cx, "Edit");
    assert!(drawn(cx, "Show password"));
    click(cx, "Show password");
    assert!(drawn(cx, "Hide password"));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!drawn(cx, "account-dialog"));

    click(cx, actions("alice"));
    click(cx, "Edit");
    assert!(drawn(cx, "Show password"));
    manager.read_with(cx, |manager, _| assert!(manager.dialog.is_some()));
}

#[gpui::test]
fn removing_asks_first(cx: &mut TestAppContext) {
    let host = headless_host();
    create(&host, None, "alice");
    create(&host, None, "bob");
    let (_, cx, _) = setup(&host, cx);
    settle(cx, |cx| drawn(cx, row("alice")));

    click(cx, actions("alice"));
    click(cx, "Remove");
    assert!(drawn(cx, "remove-confirm"));
    click(cx, "confirm-cancel");
    assert!(!drawn(cx, "remove-confirm"));
    assert_eq!(stored(&host).len(), 2);

    click(cx, actions("alice"));
    click(cx, "Remove");
    click(cx, "Remove account");
    settle(cx, |cx| !drawn(cx, row("alice")));
    assert_eq!(
        stored(&host),
        [("bob".into(), "bob".into(), "hunter2".into())]
    );
}

#[gpui::test]
fn removing_from_the_edit_dialog_closes_it(cx: &mut TestAppContext) {
    let host = headless_host();
    create(&host, None, "alice");
    let (_, cx, _) = setup(&host, cx);
    settle(cx, |cx| drawn(cx, row("alice")));

    click(cx, actions("alice"));
    click(cx, "Edit");
    click(cx, "Remove");
    assert!(drawn(cx, "remove-confirm"));
    click(cx, "confirm-cancel");
    assert!(drawn(cx, "account-dialog"));

    click(cx, "Remove");
    click(cx, "Remove account");
    settle(cx, |cx| !drawn(cx, row("alice")));
    assert!(!drawn(cx, "account-dialog"));
    assert_eq!(stored(&host), []);
}

#[gpui::test]
fn launch_names_the_account(cx: &mut TestAppContext) {
    let host = headless_host();
    create(&host, None, "alice");
    create(&host, None, "bob");
    let (_, cx, launches) = setup(&host, cx);
    settle(cx, |cx| drawn(cx, row("bob")));

    click(cx, actions("bob"));
    click(cx, "Launch");
    assert_eq!(*launches.borrow(), ["bob"]);
    assert!(!drawn(cx, "Launch"));
}

#[gpui::test]
fn a_reopened_window_keeps_its_state(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(lucent_ui::text_field::init);
    let host = headless_host();
    let first = cx.update(|cx| show(&host, cx)).unwrap();
    let mut vcx = VisualTestContext::from_window(first.into(), cx);
    settle(&mut vcx, |cx| drawn(cx, "accounts-empty"));
    click(&mut vcx, "add-account");
    vcx.simulate_input("alice");
    vcx.run_until_parked();
    let size = vcx.update(|window, _| window.viewport_size());
    assert_eq!(size, gpui::size(px(980.), px(720.)));

    vcx.update(|window, _| window.remove_window());
    cx.run_until_parked();
    let second = cx.update(|cx| show(&host, cx)).unwrap();
    assert_ne!(
        AnyWindowHandle::from(first).window_id(),
        AnyWindowHandle::from(second).window_id()
    );
    let again = cx.update(|cx| show(&host, cx)).unwrap();
    assert_eq!(
        AnyWindowHandle::from(second).window_id(),
        AnyWindowHandle::from(again).window_id()
    );

    let mut vcx = VisualTestContext::from_window(second.into(), cx);
    vcx.run_until_parked();
    assert!(drawn(&mut vcx, "account-dialog"));
    let manager = second.update(cx, |_, _, cx| cx.entity()).unwrap();
    assert_eq!(field_value(&manager, &mut vcx, Field::Username), "alice");
}
