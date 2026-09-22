"use strict";
(() => {
  const $ = (id) => document.getElementById(id);
  const messages = {
    invalid_credentials: "用户名或密码不正确，或账号当前不可用。",
    authentication_required: "登录已失效，请重新登录。",
    login_rate_limited: "尝试次数过多，请在 15 分钟后再试。",
    account_management_denied: "你没有管理他人账号的授权。",
    account_conflict: "用户名或人员已开通账号，请核对账号列表。",
    password_length: "密码须为 12–128 个字符。",
    invalid_username: "请使用 3–64 位英文字母、数字、点、下划线或连字符。",
    account_not_active: "账号已停用或不可操作，请重新查看列表。",
  };
  const tell = (text) => {
    $("auth-notice").textContent = text;
  };
  async function api(path, data) {
    const response = await fetch(
      path,
      data
        ? {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(data),
          }
        : { cache: "no-store" }
    );
    const value = await response.json();
    if (!response.ok) {
      if (
        response.status === 401 &&
        path !== "/api/login" &&
        value.code !== "invalid_credentials"
      )
        location.replace("/login");
      throw new Error(messages[value.code] || "操作未完成，请重新读取状态后核对。");
    }
    return value;
  }
  function submit(form, handler) {
    form.addEventListener("submit", async (event) => {
      event.preventDefault();
      const buttons = form.querySelectorAll("button");
      buttons.forEach((b) => (b.disabled = true));
      tell("");
      try {
        await handler(Object.fromEntries(new FormData(form)));
      } catch (e) {
        tell(e.message || "无法连接本地服务，请稍后重试。");
      } finally {
        form.querySelectorAll('input[type="password"]').forEach((i) => (i.value = ""));
        buttons.forEach((b) => (b.disabled = false));
      }
    });
  }
  if (document.body.dataset.auth === "login") {
    submit($("login-form"), async (data) => {
      await api("/api/login", data);
      location.replace("/");
    });
    return;
  }
  $("logout").onclick = async () => {
    try {
      await api("/api/logout", {});
      location.replace("/login");
    } catch (e) {
      tell(e.message);
    }
  };
  submit($("password-form"), async (data) => {
    await api("/api/password", data);
    location.replace("/login");
  });
  const element = (tag, text) => {
    const n = document.createElement(tag);
    if (text) n.textContent = text;
    return n;
  };
  async function accounts() {
    const data = await api("/api/accounts");
    const select = $("create-account").elements.person;
    select.replaceChildren();
    for (const p of data.people) {
      const o = element("option", `${p.label} · ${p.id.slice(0, 8)}`);
      o.value = p.id;
      select.append(o);
    }
    $("create-account").querySelector("button").disabled = !data.people.length;
    $("accounts").replaceChildren();
    for (const a of data.accounts) {
      const row = element("section");
      row.className = "account-row";
      row.append(
        element("h3", a.label),
        element(
          "p",
          `${a.username} · ${a.status === "active" ? "正常" : "已停用"} · 人员 ${a.person.slice(0, 8)}`
        )
      );
      if (a.status === "active") {
        const form = element("form"),
          label = element("label", "重置为新密码"),
          input = element("input");
        input.type = "password";
        input.name = "password";
        input.autocomplete = "new-password";
        input.minLength = 12;
        input.maxLength = 128;
        input.required = true;
        label.append(input);
        const reset = element("button", "重置密码");
        reset.type = "submit";
        form.append(label, reset);
        submit(form, async (d) => {
          if (!confirm(`重置 ${a.username} 的密码？其全部登录会话将失效。`)) return;
          await api("/api/accounts", {
            kind: "reset",
            account: a.id,
            password: d.password,
          });
          tell("密码已重置，旧会话已失效。");
          await accounts();
        });
        const disable = element("button", "停用账号");
        disable.className = "danger";
        disable.type = "button";
        disable.onclick = async () => {
          if (!confirm(`停用 ${a.username}？其全部登录会话将失效。`)) return;
          disable.disabled = true;
          try {
            await api("/api/accounts", { kind: "disable", account: a.id });
            tell("账号已停用，旧会话已失效。");
            await accounts();
          } catch (e) {
            tell(e.message);
            disable.disabled = false;
          }
        };
        const actions = element("div");
        actions.className = "account-actions";
        actions.append(disable);
        row.append(form, actions);
      }
      $("accounts").append(row);
    }
  }
  submit($("create-account"), async (d) => {
    await api("/api/accounts", { kind: "create", ...d });
    $("create-account").reset();
    tell("账号已开通；业务权限未增加。");
    await accounts();
  });
  api("/api/me")
    .then(async (me) => {
      $("identity").textContent = `${me.label} · 个人账号`;
      if (me.account_admin) {
        $("account-admin").hidden = false;
        await accounts();
      }
    })
    .catch((e) => tell(e.message));
})();
