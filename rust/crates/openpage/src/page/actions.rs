use super::*;

impl Actions {
    pub fn new(page: Page) -> Self {
        Self {
            page,
            curr_x: 0.0,
            curr_y: 0.0,
            modifiers: 0,
            pressed_buttons: 0,
        }
    }

    pub fn owner(&self) -> &Page {
        &self.page
    }

    pub fn curr_x(&self) -> i64 {
        self.curr_x.round() as i64
    }

    pub fn curr_y(&self) -> i64 {
        self.curr_y.round() as i64
    }

    pub fn move_to<'a, T>(
        &mut self,
        target: T,
        offset_x: Option<f64>,
        offset_y: Option<f64>,
        duration_secs: f64,
    ) -> OpenPageResult<&mut Self>
    where
        T: Into<ActionsTarget<'a>>,
    {
        let (x, y) = resolve_actions_target_point(&self.page, target.into(), offset_x, offset_y)?;
        self.move_pointer_to(x, y, duration_secs)?;
        Ok(self)
    }

    pub fn r#move(
        &mut self,
        offset_x: f64,
        offset_y: f64,
        duration_secs: f64,
    ) -> OpenPageResult<&mut Self> {
        self.move_pointer_to(
            self.curr_x + offset_x,
            self.curr_y + offset_y,
            duration_secs,
        )?;
        Ok(self)
    }

    pub fn move_by(
        &mut self,
        offset_x: f64,
        offset_y: f64,
        duration_secs: f64,
    ) -> OpenPageResult<&mut Self> {
        self.r#move(offset_x, offset_y, duration_secs)
    }

    pub fn up(&mut self, pixels: f64) -> OpenPageResult<&mut Self> {
        self.r#move(0.0, -pixels, 0.5)
    }

    pub fn down(&mut self, pixels: f64) -> OpenPageResult<&mut Self> {
        self.r#move(0.0, pixels, 0.5)
    }

    pub fn left(&mut self, pixels: f64) -> OpenPageResult<&mut Self> {
        self.r#move(-pixels, 0.0, 0.5)
    }

    pub fn right(&mut self, pixels: f64) -> OpenPageResult<&mut Self> {
        self.r#move(pixels, 0.0, 0.5)
    }

    pub fn click<'a, T>(&mut self, on_target: Option<T>, times: u32) -> OpenPageResult<&mut Self>
    where
        T: Into<ActionsTarget<'a>>,
    {
        if let Some(target) = on_target {
            self.move_to(target, None, None, 0.0)?;
        }
        self.dispatch_click(MouseButton::Left, times)?;
        Ok(self)
    }

    pub fn r_click<'a, T>(&mut self, on_target: Option<T>, times: u32) -> OpenPageResult<&mut Self>
    where
        T: Into<ActionsTarget<'a>>,
    {
        if let Some(target) = on_target {
            self.move_to(target, None, None, 0.0)?;
        }
        self.dispatch_click(MouseButton::Right, times)?;
        Ok(self)
    }

    pub fn m_click<'a, T>(&mut self, on_target: Option<T>, times: u32) -> OpenPageResult<&mut Self>
    where
        T: Into<ActionsTarget<'a>>,
    {
        if let Some(target) = on_target {
            self.move_to(target, None, None, 0.0)?;
        }
        self.dispatch_click(MouseButton::Middle, times)?;
        Ok(self)
    }

    pub fn hold<'a, T>(&mut self, on_target: Option<T>) -> OpenPageResult<&mut Self>
    where
        T: Into<ActionsTarget<'a>>,
    {
        if let Some(target) = on_target {
            self.move_to(target, None, None, 0.0)?;
        }
        self.press_button(MouseButton::Left)?;
        Ok(self)
    }

    pub fn release<'a, T>(&mut self, on_target: Option<T>) -> OpenPageResult<&mut Self>
    where
        T: Into<ActionsTarget<'a>>,
    {
        if let Some(target) = on_target {
            self.move_to(target, None, None, 0.0)?;
        }
        self.release_button(MouseButton::Left)?;
        Ok(self)
    }

    pub fn r_hold<'a, T>(&mut self, on_target: Option<T>) -> OpenPageResult<&mut Self>
    where
        T: Into<ActionsTarget<'a>>,
    {
        if let Some(target) = on_target {
            self.move_to(target, None, None, 0.0)?;
        }
        self.press_button(MouseButton::Right)?;
        Ok(self)
    }

    pub fn r_release<'a, T>(&mut self, on_target: Option<T>) -> OpenPageResult<&mut Self>
    where
        T: Into<ActionsTarget<'a>>,
    {
        if let Some(target) = on_target {
            self.move_to(target, None, None, 0.0)?;
        }
        self.release_button(MouseButton::Right)?;
        Ok(self)
    }

    pub fn m_hold<'a, T>(&mut self, on_target: Option<T>) -> OpenPageResult<&mut Self>
    where
        T: Into<ActionsTarget<'a>>,
    {
        if let Some(target) = on_target {
            self.move_to(target, None, None, 0.0)?;
        }
        self.press_button(MouseButton::Middle)?;
        Ok(self)
    }

    pub fn m_release<'a, T>(&mut self, on_target: Option<T>) -> OpenPageResult<&mut Self>
    where
        T: Into<ActionsTarget<'a>>,
    {
        if let Some(target) = on_target {
            self.move_to(target, None, None, 0.0)?;
        }
        self.release_button(MouseButton::Middle)?;
        Ok(self)
    }

    pub fn scroll<'a, T>(
        &mut self,
        delta_y: f64,
        delta_x: f64,
        on_target: Option<T>,
    ) -> OpenPageResult<&mut Self>
    where
        T: Into<ActionsTarget<'a>>,
    {
        if let Some(target) = on_target {
            self.move_to(target, None, None, 0.0)?;
        }
        let mut event = DispatchMouseEventParams::new(
            DispatchMouseEventType::MouseWheel,
            self.curr_x,
            self.curr_y,
        );
        event.buttons = Some(self.pressed_buttons);
        event.modifiers = Some(self.modifiers);
        event.delta_x = Some(delta_x);
        event.delta_y = Some(delta_y);
        self.dispatch_mouse_event(event)?;
        Ok(self)
    }

    pub fn key_down(&mut self, key: &str) -> OpenPageResult<&mut Self> {
        let definition = keys::get_key_definition(key)
            .ok_or_else(|| OpenPageError::PageOperation(unsupported_key_message(key)))?;
        let next_modifiers = self.modifiers | action_modifier_bit(definition.key).unwrap_or(0);
        self.dispatch_key_event(action_build_key_event(&definition, next_modifiers, false))?;
        self.modifiers = next_modifiers;
        Ok(self)
    }

    pub fn key_up(&mut self, key: &str) -> OpenPageResult<&mut Self> {
        let definition = keys::get_key_definition(key)
            .ok_or_else(|| OpenPageError::PageOperation(unsupported_key_message(key)))?;
        let next_modifiers = self.modifiers & !action_modifier_bit(definition.key).unwrap_or(0);
        self.dispatch_key_event(action_build_key_event(&definition, next_modifiers, true))?;
        self.modifiers = next_modifiers;
        Ok(self)
    }

    pub fn input<'a, I>(&mut self, input: I) -> OpenPageResult<&mut Self>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.perform_actions_input(input.into(), true, None)?;
        Ok(self)
    }

    pub fn r#type<'a, I>(&mut self, input: I) -> OpenPageResult<&mut Self>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.perform_actions_input(input.into(), false, None)?;
        Ok(self)
    }

    pub fn type_with_interval<'a, I>(
        &mut self,
        input: I,
        interval_secs: f64,
    ) -> OpenPageResult<&mut Self>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.perform_actions_input(input.into(), false, Some(interval_secs))?;
        Ok(self)
    }

    pub fn type_keys<'a, I>(&mut self, input: I) -> OpenPageResult<&mut Self>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.r#type(input)
    }

    pub fn type_keys_with_interval<'a, I>(
        &mut self,
        input: I,
        interval_secs: f64,
    ) -> OpenPageResult<&mut Self>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.type_with_interval(input, interval_secs)
    }

    pub fn wait(&mut self, second: f64, scope: Option<f64>) -> OpenPageResult<&mut Self> {
        if second.is_sign_negative() {
            return Err(OpenPageError::PageOperation(
                action_wait_seconds_non_negative_message(),
            ));
        }
        let wait_secs = match scope {
            Some(end) => action_wait_duration_secs(second, end),
            None => second,
        };
        sleep(Duration::from_secs_f64(wait_secs.max(0.0)));
        Ok(self)
    }

    pub fn drag_in<'a, T>(
        &mut self,
        target: T,
        data: ActionsDragData<'a>,
    ) -> OpenPageResult<&mut Self>
    where
        T: Into<ActionsTarget<'a>>,
    {
        let (x, y) = resolve_actions_target_point(&self.page, target.into(), None, None)?;
        let payload = action_drag_payload(data)?;
        self.dispatch_drag_event("dragEnter", x, y, payload.clone())?;
        self.dispatch_drag_event("dragOver", x, y, payload.clone())?;
        self.dispatch_drag_event("drop", x, y, payload)?;
        Ok(self)
    }

    fn perform_actions_input(
        &mut self,
        input: ActionsInput<'_>,
        prefer_insert_text: bool,
        interval_secs: Option<f64>,
    ) -> OpenPageResult<()> {
        let values = actions_input_values(input);
        if values.is_empty() {
            return Ok(());
        }

        if let Some(interval_secs) = interval_secs {
            if interval_secs.is_sign_negative() {
                return Err(OpenPageError::PageOperation(
                    action_type_interval_non_negative_message(),
                ));
            }
        }

        let mut transient_keys = Vec::new();
        let result = (|| -> OpenPageResult<()> {
            for value in values {
                if value.is_empty() {
                    continue;
                }
                if action_modifier_bit(&value).is_some() {
                    self.key_down(&value)?;
                    transient_keys.push(value);
                    continue;
                }
                let effective_modifiers = self.modifiers;
                if keys::get_key_definition(&value).is_some() {
                    let effective_value = action_effective_key_value(&value, effective_modifiers);
                    self.press_key_value(effective_value.as_ref(), effective_modifiers)?;
                    action_sleep_interval(interval_secs);
                    continue;
                }
                if effective_modifiers != 0 {
                    for ch in value.chars() {
                        let key = ch.to_string();
                        let effective_value = action_effective_key_value(&key, effective_modifiers);
                        self.press_key_value(effective_value.as_ref(), effective_modifiers)?;
                        action_sleep_interval(interval_secs);
                    }
                    continue;
                }
                if prefer_insert_text {
                    self.insert_text_value(&value)?;
                    action_sleep_interval(interval_secs);
                } else {
                    self.type_text_value(&value, interval_secs)?;
                }
            }
            Ok(())
        })();

        let mut cleanup_error = None;
        for key in transient_keys {
            if let Err(err) = self.key_up(&key) {
                if cleanup_error.is_none() {
                    cleanup_error = Some(err);
                }
            }
        }

        match (result, cleanup_error) {
            (Err(err), _) => Err(err),
            (Ok(()), Some(err)) => Err(err),
            (Ok(()), None) => Ok(()),
        }
    }

    fn move_pointer_to(
        &mut self,
        target_x: f64,
        target_y: f64,
        duration_secs: f64,
    ) -> OpenPageResult<()> {
        let path = action_move_path(self.curr_x, self.curr_y, target_x, target_y, duration_secs);
        let pause = action_move_pause(duration_secs, path.len());
        let path_len = path.len();
        for (index, point) in path.into_iter().enumerate() {
            let mut moved =
                DispatchMouseEventParams::new(DispatchMouseEventType::MouseMoved, point.x, point.y);
            moved.buttons = Some(self.pressed_buttons);
            moved.modifiers = Some(self.modifiers);
            self.dispatch_mouse_event(moved)?;
            self.curr_x = point.x;
            self.curr_y = point.y;
            if index + 1 < path_len {
                if let Some(pause) = pause {
                    sleep(pause);
                }
            }
        }
        Ok(())
    }

    fn press_button(&mut self, button: MouseButton) -> OpenPageResult<()> {
        let next_buttons = self.pressed_buttons | action_mouse_buttons(&button);
        let mut pressed = DispatchMouseEventParams::new(
            DispatchMouseEventType::MousePressed,
            self.curr_x,
            self.curr_y,
        );
        pressed.button = Some(button);
        pressed.buttons = Some(next_buttons);
        pressed.modifiers = Some(self.modifiers);
        pressed.click_count = Some(1);
        self.dispatch_mouse_event(pressed)?;
        self.pressed_buttons = next_buttons;
        Ok(())
    }

    fn release_button(&mut self, button: MouseButton) -> OpenPageResult<()> {
        let next_buttons = self.pressed_buttons & !action_mouse_buttons(&button);
        let mut released = DispatchMouseEventParams::new(
            DispatchMouseEventType::MouseReleased,
            self.curr_x,
            self.curr_y,
        );
        released.button = Some(button);
        released.buttons = Some(next_buttons);
        released.modifiers = Some(self.modifiers);
        released.click_count = Some(1);
        self.dispatch_mouse_event(released)?;
        self.pressed_buttons = next_buttons;
        Ok(())
    }

    fn dispatch_click(&mut self, button: MouseButton, times: u32) -> OpenPageResult<()> {
        if times == 0 {
            return Err(OpenPageError::PageOperation(
                action_click_times_positive_message(),
            ));
        }
        let pressed_buttons = self.pressed_buttons | action_mouse_buttons(&button);
        for click_count in 1..=times {
            let mut pressed = DispatchMouseEventParams::new(
                DispatchMouseEventType::MousePressed,
                self.curr_x,
                self.curr_y,
            );
            pressed.button = Some(button.clone());
            pressed.buttons = Some(pressed_buttons);
            pressed.modifiers = Some(self.modifiers);
            pressed.click_count = Some(click_count.into());
            self.dispatch_mouse_event(pressed)?;

            let mut released = DispatchMouseEventParams::new(
                DispatchMouseEventType::MouseReleased,
                self.curr_x,
                self.curr_y,
            );
            released.button = Some(button.clone());
            released.buttons = Some(self.pressed_buttons);
            released.modifiers = Some(self.modifiers);
            released.click_count = Some(click_count.into());
            self.dispatch_mouse_event(released)?;
        }
        Ok(())
    }

    fn dispatch_mouse_event(&self, event: DispatchMouseEventParams) -> OpenPageResult<()> {
        execute_page_command_blocking(
            self.page.runtime.as_ref(),
            &self.page.inner,
            event,
            "Actions::dispatch_mouse_event()",
        )?;
        Ok(())
    }

    fn dispatch_key_event(&self, event: DispatchKeyEventParams) -> OpenPageResult<()> {
        execute_page_command_blocking(
            self.page.runtime.as_ref(),
            &self.page.inner,
            event,
            "Actions::dispatch_key_event()",
        )?;
        Ok(())
    }

    fn dispatch_drag_event(
        &self,
        event_type: &'static str,
        x: f64,
        y: f64,
        data: ActionsDragPayload,
    ) -> OpenPageResult<()> {
        let event = DispatchDragEventParams {
            event_type,
            x,
            y,
            data,
            modifiers: Some(self.modifiers),
        };
        execute_page_command_blocking(
            self.page.runtime.as_ref(),
            &self.page.inner,
            event,
            "Actions::dispatch_drag_event()",
        )?;
        Ok(())
    }

    fn insert_text_value(&self, value: &str) -> OpenPageResult<()> {
        execute_page_command_blocking(
            self.page.runtime.as_ref(),
            &self.page.inner,
            InsertTextParams::new(value.to_string()),
            "Actions::insert_text_value()",
        )?;
        Ok(())
    }

    fn type_text_value(&self, value: &str, interval_secs: Option<f64>) -> OpenPageResult<()> {
        for ch in value.chars() {
            self.press_key_value(ch.to_string().as_str(), self.modifiers)?;
            action_sleep_interval(interval_secs);
        }
        Ok(())
    }

    fn press_key_value(&self, value: &str, modifiers: i64) -> OpenPageResult<()> {
        let definition = keys::get_key_definition(value)
            .ok_or_else(|| OpenPageError::PageOperation(unsupported_key_message(value)))?;
        self.dispatch_key_event(action_build_key_event(&definition, modifiers, false))?;
        self.dispatch_key_event(action_build_key_event(&definition, modifiers, true))
    }
}
