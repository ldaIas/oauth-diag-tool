// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0
//
// Bridge between Elm ports and Tauri commands. Every command failure is
// reported to the UI via the notifyError port (or a flow-specific port).

var app = Elm.Main.init({ node: document.getElementById("app") });

function waitForTauri(callback) {
  if (window.__TAURI__) {
    callback();
    return;
  }
  var interval = setInterval(function () {
    if (window.__TAURI__) {
      clearInterval(interval);
      callback();
    }
  }, 50);
}

function reportError(context, err) {
  console.error(context + " failed:", err);
  app.ports.notifyError.send(context + ": " + String(err));
}

async function checkForUpdates() {
  try {
    var check = await window.__TAURI__.updater.check();
    if (check && check.available) {
      var accepted = await window.__TAURI__.dialog.confirm(
        "A new version (" + check.version + ") is available. Would you like to update now?",
        { title: "Update Available", kind: "info" }
      );
      if (accepted) {
        await check.downloadAndInstall();
        await window.__TAURI__.process.relaunch();
      }
    }
  } catch (err) {
    console.error("Update check failed:", err);
  }
}

waitForTauri(checkForUpdates);

function loadServerConfigs() {
  window.__TAURI__.core.invoke("get_server_configs").then(function (configs) {
    app.ports.receiveServerConfigs.send(configs);
  }).catch(function (err) { reportError("get_server_configs", err); });
}

function loadClientConfigs() {
  window.__TAURI__.core.invoke("get_client_configs").then(function (configs) {
    app.ports.receiveClientConfigs.send(configs);
  }).catch(function (err) { reportError("get_client_configs", err); });
}

app.ports.requestServerConfigs.subscribe(function () {
  waitForTauri(loadServerConfigs);
});

app.ports.requestClientConfigs.subscribe(function () {
  waitForTauri(loadClientConfigs);
});

app.ports.deleteServerConfig.subscribe(function (data) {
  waitForTauri(function () {
    window.__TAURI__.core.invoke("delete_server_config", { id: data.id }).then(function () {
      loadServerConfigs();
    }).catch(function (err) { reportError("delete_server_config", err); });
  });
});

app.ports.createServerConfig.subscribe(function (config) {
  waitForTauri(function () {
    window.__TAURI__.core.invoke("create_server_config", {
      configName: config.configName,
      authServerUrl: config.authServerUrl,
      tokenUrl: config.tokenUrl
    }).then(function () {
      loadServerConfigs();
    }).catch(function (err) { reportError("create_server_config", err); });
  });
});

app.ports.addClientToServer.subscribe(function (data) {
  waitForTauri(function () {
    window.__TAURI__.core.invoke("add_client_to_server", { authServerId: data.authServerId }).then(function () {
      loadServerConfigs();
    }).catch(function (err) { reportError("add_client_to_server", err); });
  });
});

app.ports.deleteClient.subscribe(function (data) {
  waitForTauri(function () {
    window.__TAURI__.core.invoke("delete_client", { id: data.id }).then(function () {
      loadServerConfigs();
    }).catch(function (err) { reportError("delete_client", err); });
  });
});

app.ports.createClientConfig.subscribe(function (data) {
  waitForTauri(function () {
    window.__TAURI__.core.invoke("create_client_config", { config: data.config }).then(function () {
      loadClientConfigs();
    }).catch(function (err) { reportError("create_client_config", err); });
  });
});

app.ports.updateClientConfig.subscribe(function (data) {
  waitForTauri(function () {
    window.__TAURI__.core.invoke("update_client_config", { id: data.id, config: data.config }).then(function () {
      loadClientConfigs();
    }).catch(function (err) { reportError("update_client_config", err); });
  });
});

app.ports.deleteClientConfig.subscribe(function (data) {
  waitForTauri(function () {
    window.__TAURI__.core.invoke("delete_client_config", { id: data.id }).then(function () {
      loadClientConfigs();
    }).catch(function (err) { reportError("delete_client_config", err); });
  });
});

app.ports.fetchServerMetadata.subscribe(function (data) {
  waitForTauri(function () {
    window.__TAURI__.core.invoke("fetch_server_metadata", {
      issuerUrl: data.issuerUrl
    }).then(function (result) {
      result.configId = data.configId;
      app.ports.receiveServerMetadata.send(result);
    }).catch(function (err) {
      console.error("fetch_server_metadata failed:", err);
      app.ports.receiveServerMetadata.send({
        configId: data.configId,
        error: String(err)
      });
    });
  });
});

app.ports.updateServerSettings.subscribe(function (data) {
  waitForTauri(function () {
    window.__TAURI__.core.invoke("update_server_settings", {
      id: data.id,
      redirectUrlOverride: data.redirectUrlOverride,
      accessTokenExpiry: data.accessTokenExpiry,
      refreshTokenExpiry: data.refreshTokenExpiry
    }).then(function () {
      app.ports.serverSettingsSaved.send(data.id);
      loadServerConfigs();
    }).catch(function (err) { reportError("update_server_settings", err); });
  });
});

app.ports.startServer.subscribe(function (data) {
  waitForTauri(function () {
    window.__TAURI__.core.invoke("start_server", { id: data.id }).then(function () {
      loadServerConfigs();
    }).catch(function (err) { reportError("start_server", err); });
  });
});

app.ports.stopServer.subscribe(function (data) {
  waitForTauri(function () {
    window.__TAURI__.core.invoke("stop_server", { id: data.id }).then(function () {
      loadServerConfigs();
    }).catch(function (err) { reportError("stop_server", err); });
  });
});

app.ports.requestCallbackUrl.subscribe(function () {
  waitForTauri(function () {
    window.__TAURI__.core.invoke("get_callback_url").then(function (url) {
      app.ports.receiveCallbackUrl.send(url);
    }).catch(function (err) { reportError("get_callback_url", err); });
  });
});

app.ports.cancelAuthorization.subscribe(function (data) {
  waitForTauri(function () {
    window.__TAURI__.core.invoke("cancel_authorization", { id: data.id })
      .catch(function (err) { reportError("cancel_authorization", err); });
  });
});

waitForTauri(function () {
  window.__TAURI__.event.listen("resource-access", function (event) {
    app.ports.receiveResourceAccess.send(event.payload);
  });
});

function prettyPrintBody(result) {
  try {
    result.body = JSON.stringify(JSON.parse(result.body), null, 2);
  } catch (e) {
    // body is not JSON; leave it as-is
  }
  return result;
}

app.ports.refreshToken.subscribe(function (data) {
  waitForTauri(function () {
    window.__TAURI__.core.invoke("refresh_token", { id: data.id }).then(function (result) {
      result.configId = data.id;
      app.ports.receiveAuthResult.send(prettyPrintBody(result));
    }).catch(function (err) {
      console.error("refresh_token failed:", err);
      app.ports.receiveAuthResult.send({ configId: data.id, error: String(err) });
    });
  });
});

app.ports.authorizeClient.subscribe(function (data) {
  waitForTauri(function () {
    window.__TAURI__.core.invoke("authorize_client", { id: data.id }).then(function (result) {
      result.configId = data.id;
      app.ports.receiveAuthResult.send(prettyPrintBody(result));
    }).catch(function (err) {
      console.error("authorize_client failed:", err);
      app.ports.receiveAuthResult.send({ configId: data.id, error: String(err) });
    });
  });
});
