-- Copyright 2026 Sam Sovereign
-- SPDX-License-Identifier: Apache-2.0


module ServerList exposing (Action(..), Client, Model, Msg(..), OAuthServer, init, serverConfigDecoder, update, view)

import Html exposing (..)
import Html.Attributes exposing (class, disabled, placeholder, type_, value)
import Html.Events exposing (onClick, onInput)
import Json.Decode as Decode
import Set exposing (Set)


-- MODEL


type alias Client =
    { id : String
    , clientId : String
    , clientSecret : String
    }


type alias OAuthServer =
    { id : String
    , name : String
    , issuerUrl : String
    , authorizationUrl : String
    , tokenEndpoint : String
    , port_ : Int
    , running : Bool
    , clients : List Client
    , redirectUrlOverride : String
    , accessTokenExpiry : Int
    , refreshTokenExpiry : Int
    }


type alias SettingsEdit =
    { redirectUrlOverride : String
    , accessTokenExpiry : String
    , refreshTokenExpiry : String
    }


type alias Model =
    { servers : List OAuthServer
    , expandedClients : Set String
    , expandedSettings : Set String
    , settingsEdits : List ( String, SettingsEdit )
    , savedSettings : Set String
    }


init : Model
init =
    { servers = []
    , expandedClients = Set.empty
    , expandedSettings = Set.empty
    , settingsEdits = []
    , savedSettings = Set.empty
    }



-- UPDATE


type Msg
    = OpenCreateForm
    | DeleteServer String
    | AddClient String
    | DeleteClient String
    | ImportClient OAuthServer Client
    | StartServer String
    | StopServer String
    | ToggleClients String
    | ToggleSettings String
    | EditRedirectOverride String String
    | EditAccessTokenExpiry String String
    | EditRefreshTokenExpiry String String
    | SaveSettings String
    | SettingsSaved String
    | ClearSettingsSaved String
    | GotServerConfigs (Result Decode.Error (List OAuthServer))


type Action
    = RequestCreateForm Int
    | RequestDeleteServer String
    | RequestAddClient String
    | RequestDeleteClient String
    | RequestImportClient { name : String, issuerUrl : String, authorizationUrl : String, tokenUrl : String, clientId : String, clientSecret : String }
    | RequestStartServer String
    | RequestStopServer String
    | RequestSaveSettings { id : String, redirectUrlOverride : String, accessTokenExpiry : Int, refreshTokenExpiry : Int }
    | NoAction


update : Msg -> Model -> ( Model, Action )
update msg model =
    case msg of
        OpenCreateForm ->
            ( model, RequestCreateForm (nextPort model) )

        DeleteServer id ->
            ( model, RequestDeleteServer id )

        AddClient serverId ->
            ( { model | expandedClients = Set.insert serverId model.expandedClients }
            , RequestAddClient serverId
            )

        DeleteClient clientId ->
            ( model, RequestDeleteClient clientId )

        ImportClient server client ->
            ( model
            , RequestImportClient
                { name = server.name ++ " - " ++ client.clientId
                , issuerUrl = server.issuerUrl
                , authorizationUrl = server.authorizationUrl
                , tokenUrl = server.tokenEndpoint
                , clientId = client.clientId
                , clientSecret = client.clientSecret
                }
            )

        StartServer id ->
            ( model, RequestStartServer id )

        StopServer id ->
            ( model, RequestStopServer id )

        ToggleClients serverId ->
            let
                newExpanded =
                    if Set.member serverId model.expandedClients then
                        Set.remove serverId model.expandedClients

                    else
                        Set.insert serverId model.expandedClients
            in
            ( { model | expandedClients = newExpanded }, NoAction )

        ToggleSettings serverId ->
            let
                newExpanded =
                    if Set.member serverId model.expandedSettings then
                        Set.remove serverId model.expandedSettings

                    else
                        Set.insert serverId model.expandedSettings

                -- Initialize edit state from server data if expanding
                newEdits =
                    if not (Set.member serverId model.expandedSettings) then
                        case List.filter (\s -> s.id == serverId) model.servers of
                            server :: _ ->
                                ( serverId
                                , { redirectUrlOverride = server.redirectUrlOverride
                                  , accessTokenExpiry = String.fromInt server.accessTokenExpiry
                                  , refreshTokenExpiry = String.fromInt server.refreshTokenExpiry
                                  }
                                )
                                    :: List.filter (\( id, _ ) -> id /= serverId) model.settingsEdits

                            [] ->
                                model.settingsEdits

                    else
                        List.filter (\( id, _ ) -> id /= serverId) model.settingsEdits
            in
            ( { model | expandedSettings = newExpanded, settingsEdits = newEdits }, NoAction )

        EditRedirectOverride serverId val ->
            ( { model | settingsEdits = updateSettingsEdit serverId (\e -> { e | redirectUrlOverride = val }) model.settingsEdits }, NoAction )

        EditAccessTokenExpiry serverId val ->
            ( { model | settingsEdits = updateSettingsEdit serverId (\e -> { e | accessTokenExpiry = val }) model.settingsEdits }, NoAction )

        EditRefreshTokenExpiry serverId val ->
            ( { model | settingsEdits = updateSettingsEdit serverId (\e -> { e | refreshTokenExpiry = val }) model.settingsEdits }, NoAction )

        SettingsSaved serverId ->
            ( { model | savedSettings = Set.insert serverId model.savedSettings }, NoAction )

        ClearSettingsSaved serverId ->
            ( { model | savedSettings = Set.remove serverId model.savedSettings }, NoAction )

        SaveSettings serverId ->
            case getSettingsEdit serverId model.settingsEdits of
                Just edit ->
                    ( model
                    , RequestSaveSettings
                        { id = serverId
                        , redirectUrlOverride = edit.redirectUrlOverride
                        , accessTokenExpiry = String.toInt edit.accessTokenExpiry |> Maybe.withDefault 3600
                        , refreshTokenExpiry = String.toInt edit.refreshTokenExpiry |> Maybe.withDefault 86400
                        }
                    )

                Nothing ->
                    ( model, NoAction )

        GotServerConfigs result ->
            case result of
                Ok configs ->
                    ( { model | servers = configs }, NoAction )

                Err _ ->
                    ( model, NoAction )


updateSettingsEdit : String -> (SettingsEdit -> SettingsEdit) -> List ( String, SettingsEdit ) -> List ( String, SettingsEdit )
updateSettingsEdit serverId fn edits =
    List.map
        (\( id, edit ) ->
            if id == serverId then
                ( id, fn edit )

            else
                ( id, edit )
        )
        edits


getSettingsEdit : String -> List ( String, SettingsEdit ) -> Maybe SettingsEdit
getSettingsEdit serverId edits =
    List.filterMap
        (\( id, edit ) ->
            if id == serverId then
                Just edit

            else
                Nothing
        )
        edits
        |> List.head



-- PORTS / HELPERS


nextPort : Model -> Int
nextPort model =
    let
        basePort =
            9500

        usedPorts =
            List.map .port_ model.servers
    in
    findAvailable basePort usedPorts


findAvailable : Int -> List Int -> Int
findAvailable candidate used =
    if List.member candidate used then
        findAvailable (candidate + 1) used

    else
        candidate



-- DECODERS


clientDecoder : Decode.Decoder Client
clientDecoder =
    Decode.map3 Client
        (Decode.field "id" Decode.int |> Decode.map String.fromInt)
        (Decode.field "clientId" Decode.string)
        (Decode.field "clientSecret" Decode.string)


serverConfigDecoder : Decode.Decoder OAuthServer
serverConfigDecoder =
    Decode.field "id" Decode.int
        |> Decode.andThen
            (\id ->
                Decode.map8
                    (\name authUrl tokUrl running clients redirectOverride accessExpiry refreshExpiry ->
                        let
                            port_ =
                                extractPort authUrl
                        in
                        { id = String.fromInt id
                        , name = name
                        , issuerUrl = "http://localhost:" ++ String.fromInt port_
                        , authorizationUrl = authUrl
                        , tokenEndpoint = tokUrl
                        , port_ = port_
                        , running = running
                        , clients = clients
                        , redirectUrlOverride = redirectOverride
                        , accessTokenExpiry = accessExpiry
                        , refreshTokenExpiry = refreshExpiry
                        }
                    )
                    (Decode.field "configName" Decode.string)
                    (Decode.field "authServerUrl" Decode.string)
                    (Decode.field "tokenUrl" Decode.string)
                    (Decode.field "running" Decode.bool)
                    (Decode.field "clients" (Decode.list clientDecoder))
                    (Decode.field "redirectUrlOverride" Decode.string)
                    (Decode.field "accessTokenExpiry" Decode.int)
                    (Decode.field "refreshTokenExpiry" Decode.int)
            )


extractPort : String -> Int
extractPort url =
    url
        |> String.replace "http://localhost:" ""
        |> String.split "/"
        |> List.head
        |> Maybe.andThen String.toInt
        |> Maybe.withDefault 9500



-- VIEW


view : Model -> Html Msg
view model =
    div [ class "page-content" ]
        [ viewBody model
        , viewFooter
        ]


viewBody : Model -> Html Msg
viewBody model =
    if List.isEmpty model.servers then
        div [ class "empty-state" ]
            [ span [ class "empty-label" ] [ text "No servers configured" ] ]

    else
        div [ class "server-list" ]
            (List.map (viewServerCard model) model.servers)


viewServerCard : Model -> OAuthServer -> Html Msg
viewServerCard model server =
    let
        isClientsExpanded =
            Set.member server.id model.expandedClients

        isSettingsExpanded =
            Set.member server.id model.expandedSettings

        clientCount =
            List.length server.clients

        clientChevron =
            if isClientsExpanded then
                "\u{25BC}"

            else
                "\u{25B6}"

        settingsChevron =
            if isSettingsExpanded then
                "\u{25BC}"

            else
                "\u{25B6}"
    in
    div [ class "server-card" ]
        [ div [ class "server-card-header" ]
            [ div [ class "server-card-left" ]
                [ span [ class "server-name" ] [ text server.name ]
                , span [ class "server-meta" ] [ text ("localhost:" ++ String.fromInt server.port_) ]
                ]
            , div [ class "server-card-right" ]
                [ viewServerControls server
                , div [ class ("status-dot" ++ statusClass server.running) ] []
                , span [ class "status-label" ] [ text (statusLabel server.running) ]
                , button
                    [ class "btn-delete"
                    , onClick (DeleteServer server.id)
                    , disabled server.running
                    ]
                    [ text "\u{1F5D1}" ]
                ]
            ]
        , div [ class "server-card-details" ]
            [ viewDetail "Issuer" server.issuerUrl
            , viewDetail "Authorization" server.authorizationUrl
            , viewDetail "Token" server.tokenEndpoint
            ]
        , div [ class "settings-section" ]
            [ div [ class "settings-section-header" ]
                [ span [ class "expandable-toggle", onClick (ToggleSettings server.id) ]
                    [ text (settingsChevron ++ " Settings") ]
                ]
            , if isSettingsExpanded then
                viewSettingsForm server.id model.savedSettings model.settingsEdits

              else
                text ""
            ]
        , div [ class "client-section" ]
            [ div [ class "client-section-header" ]
                [ span [ class "expandable-toggle", onClick (ToggleClients server.id) ]
                    [ text (clientChevron ++ " Clients (" ++ String.fromInt clientCount ++ ")") ]
                , button [ class "btn-add-client", onClick (AddClient server.id) ] [ text "+ Add Client" ]
                ]
            , if isClientsExpanded then
                if List.isEmpty server.clients then
                    div [ class "client-empty" ] [ text "No clients" ]

                else
                    div [ class "client-list" ]
                        (List.map (viewClientCard server) server.clients)

              else
                text ""
            ]
        ]


viewSettingsForm : String -> Set String -> List ( String, SettingsEdit ) -> Html Msg
viewSettingsForm serverId saved edits =
    case getSettingsEdit serverId edits of
        Just edit ->
            let
                isSaved =
                    Set.member serverId saved
            in
            div [ class "settings-form" ]
                [ div [ class "settings-hint" ] [ text "Server must be restarted for changes to take effect." ]
                , div [ class "settings-field" ]
                    [ span [ class "detail-label" ] [ text "Redirect URL Override" ]
                    , input
                        [ class "form-input settings-input"
                        , type_ "text"
                        , placeholder "Leave empty to use client-provided URL"
                        , value edit.redirectUrlOverride
                        , onInput (EditRedirectOverride serverId)
                        ]
                        []
                    ]
                , div [ class "settings-field" ]
                    [ span [ class "detail-label" ] [ text "Access Token Expiry (seconds)" ]
                    , input
                        [ class "form-input settings-input"
                        , type_ "number"
                        , placeholder "3600"
                        , value edit.accessTokenExpiry
                        , onInput (EditAccessTokenExpiry serverId)
                        ]
                        []
                    ]
                , div [ class "settings-field" ]
                    [ span [ class "detail-label" ] [ text "Refresh Token Expiry (seconds)" ]
                    , input
                        [ class "form-input settings-input"
                        , type_ "number"
                        , placeholder "86400"
                        , value edit.refreshTokenExpiry
                        , onInput (EditRefreshTokenExpiry serverId)
                        ]
                        []
                    ]
                , div [ class "settings-actions" ]
                    [ if isSaved then
                        span [ class "settings-saved-label" ] [ text "\u{2713} Saved" ]

                      else
                        text ""
                    , button
                        [ class "btn-save-settings"
                        , disabled isSaved
                        , onClick (SaveSettings serverId)
                        ]
                        [ text "Save" ]
                    ]
                ]

        Nothing ->
            text ""


viewServerControls : OAuthServer -> Html Msg
viewServerControls server =
    if server.running then
        button [ class "btn-server-control btn-stop", onClick (StopServer server.id) ]
            [ text "\u{25A0} Stop" ]

    else
        button [ class "btn-server-control btn-start", onClick (StartServer server.id) ]
            [ text "\u{25B6} Start" ]


viewClientCard : OAuthServer -> Client -> Html Msg
viewClientCard server client =
    div [ class "client-card" ]
        [ div [ class "client-card-header" ]
            [ span [ class "client-label" ] [ text ("Client " ++ client.id) ]
            , div [ class "client-card-actions" ]
                [ button [ class "btn-import", onClick (ImportClient server client) ] [ text "Import" ]
                , button [ class "btn-delete", onClick (DeleteClient client.id) ] [ text "\u{1F5D1}" ]
                ]
            ]
        , div [ class "client-details" ]
            [ viewDetail "Client ID" client.clientId
            , viewDetail "Secret" client.clientSecret
            ]
        ]


viewDetail : String -> String -> Html Msg
viewDetail label val =
    div [ class "server-detail" ]
        [ span [ class "detail-label" ] [ text label ]
        , span [ class "detail-value" ] [ text val ]
        ]


statusClass : Bool -> String
statusClass running =
    if running then
        " running"

    else
        ""


statusLabel : Bool -> String
statusLabel running =
    if running then
        "running"

    else
        "stopped"


viewFooter : Html Msg
viewFooter =
    div [ class "footer" ]
        [ button [ class "btn-create", onClick OpenCreateForm ]
            [ text "+ Create new OAuth Server" ]
        ]
