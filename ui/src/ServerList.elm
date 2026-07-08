-- Copyright 2026 Sam Sovereign
-- SPDX-License-Identifier: Apache-2.0


module ServerList exposing (Action(..), Client, Model, Msg(..), OAuthServer, ResourceAccess, init, resourceAccessDecoder, serverConfigDecoder, update, view)

import Dict exposing (Dict)
import Html exposing (..)
import Html.Attributes exposing (class, disabled, placeholder, type_, value)
import Html.Events exposing (onClick, onInput)
import Json.Decode as Decode
import Set exposing (Set)


-- MODEL


type alias Client =
    { id : Int
    , clientId : String
    , clientSecret : String
    }


type alias ResourceAccess =
    { serverId : Int
    , clientId : String
    , status : Int
    , error : Maybe String
    , timestamp : String
    }


type alias OAuthServer =
    { id : Int
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
    , lastResourceAccess : Maybe ResourceAccess
    }


type alias SettingsEdit =
    { redirectUrlOverride : String
    , accessTokenExpiry : String
    , refreshTokenExpiry : String
    }


type alias Model =
    { servers : List OAuthServer
    , expandedClients : Set Int
    , expandedSettings : Set Int
    , settingsEdits : Dict Int SettingsEdit
    , savedSettings : Set Int
    }


init : Model
init =
    { servers = []
    , expandedClients = Set.empty
    , expandedSettings = Set.empty
    , settingsEdits = Dict.empty
    , savedSettings = Set.empty
    }



-- UPDATE


type Msg
    = OpenCreateForm
    | DeleteServer Int
    | AddClient Int
    | DeleteClient Int
    | ImportClient OAuthServer Client
    | StartServer Int
    | StopServer Int
    | ToggleClients Int
    | ToggleSettings Int
    | EditRedirectOverride Int String
    | EditAccessTokenExpiry Int String
    | EditRefreshTokenExpiry Int String
    | SaveSettings Int
    | SettingsSaved Int
    | ClearSettingsSaved Int
    | GotServerConfigs (List OAuthServer)
    | GotResourceAccess ResourceAccess


type Action
    = RequestCreateForm Int
    | RequestDeleteServer Int
    | RequestAddClient Int
    | RequestDeleteClient Int
    | RequestImportClient { name : String, issuerUrl : String, authorizationUrl : String, tokenUrl : String, clientId : String, clientSecret : String }
    | RequestStartServer Int
    | RequestStopServer Int
    | RequestSaveSettings { id : Int, redirectUrlOverride : String, accessTokenExpiry : Int, refreshTokenExpiry : Int }
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
            ( { model | expandedClients = toggleMember serverId model.expandedClients }, NoAction )

        ToggleSettings serverId ->
            let
                expanding =
                    not (Set.member serverId model.expandedSettings)

                -- Initialize edit state from server data when expanding
                newEdits =
                    if expanding then
                        case List.filter (\s -> s.id == serverId) model.servers of
                            server :: _ ->
                                Dict.insert serverId
                                    { redirectUrlOverride = server.redirectUrlOverride
                                    , accessTokenExpiry = String.fromInt server.accessTokenExpiry
                                    , refreshTokenExpiry = String.fromInt server.refreshTokenExpiry
                                    }
                                    model.settingsEdits

                            [] ->
                                model.settingsEdits

                    else
                        Dict.remove serverId model.settingsEdits
            in
            ( { model
                | expandedSettings = toggleMember serverId model.expandedSettings
                , settingsEdits = newEdits
              }
            , NoAction
            )

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
            case Dict.get serverId model.settingsEdits of
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

        GotServerConfigs configs ->
            ( { model | servers = configs }, NoAction )

        GotResourceAccess access ->
            ( { model
                | servers =
                    List.map
                        (\s ->
                            if s.id == access.serverId then
                                { s | lastResourceAccess = Just access }

                            else
                                s
                        )
                        model.servers
              }
            , NoAction
            )


toggleMember : Int -> Set Int -> Set Int
toggleMember id set =
    if Set.member id set then
        Set.remove id set

    else
        Set.insert id set


updateSettingsEdit : Int -> (SettingsEdit -> SettingsEdit) -> Dict Int SettingsEdit -> Dict Int SettingsEdit
updateSettingsEdit serverId fn edits =
    Dict.update serverId (Maybe.map fn) edits



-- HELPERS


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
        (Decode.field "id" Decode.int)
        (Decode.field "clientId" Decode.string)
        (Decode.field "clientSecret" Decode.string)


resourceAccessDecoder : Decode.Decoder ResourceAccess
resourceAccessDecoder =
    Decode.map5 ResourceAccess
        (Decode.field "serverId" Decode.int)
        (Decode.field "clientId" Decode.string)
        (Decode.field "status" Decode.int)
        (Decode.field "error" (Decode.nullable Decode.string))
        (Decode.field "timestamp" Decode.string)


serverConfigDecoder : Decode.Decoder OAuthServer
serverConfigDecoder =
    Decode.field "id" Decode.int
        |> Decode.andThen
            (\id ->
                Decode.map8
                    (\name issuer authUrl tokUrl port_ running clients ( redirectOverride, accessExpiry, refreshExpiry ) ->
                        { id = id
                        , name = name
                        , issuerUrl = issuer
                        , authorizationUrl = authUrl
                        , tokenEndpoint = tokUrl
                        , port_ = port_
                        , running = running
                        , clients = clients
                        , redirectUrlOverride = redirectOverride
                        , accessTokenExpiry = accessExpiry
                        , refreshTokenExpiry = refreshExpiry
                        , lastResourceAccess = Nothing
                        }
                    )
                    (Decode.field "configName" Decode.string)
                    (Decode.field "issuerUrl" Decode.string)
                    (Decode.field "authServerUrl" Decode.string)
                    (Decode.field "tokenUrl" Decode.string)
                    (Decode.field "port" Decode.int)
                    (Decode.field "running" Decode.bool)
                    (Decode.field "clients" (Decode.list clientDecoder))
                    (Decode.map3 (\a b c -> ( a, b, c ))
                        (Decode.field "redirectUrlOverride" Decode.string)
                        (Decode.field "accessTokenExpiry" Decode.int)
                        (Decode.field "refreshTokenExpiry" Decode.int)
                    )
            )



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
        , viewResourceSection server
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


viewSettingsForm : Int -> Set Int -> Dict Int SettingsEdit -> Html Msg
viewSettingsForm serverId saved edits =
    case Dict.get serverId edits of
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


viewResourceSection : OAuthServer -> Html Msg
viewResourceSection server =
    div [ class "resource-section" ]
        [ div [ class "resource-section-header" ]
            [ span [ class "section-label" ] [ text "Resource" ] ]
        , div [ class "resource-hint" ]
            [ text ("Test your token by doing a request: GET " ++ server.issuerUrl ++ "/resource") ]
        , div [ class "resource-hint" ]
            [ text ("HINT: Add a \"resource\" parameter to your request when getting a token equal to " ++ server.issuerUrl ++ "/resource") ]
        , case server.lastResourceAccess of
            Just access ->
                div [ class "resource-details" ]
                    [ viewDetail "Last Client"
                        (if String.isEmpty access.clientId then
                            "—"

                         else
                            access.clientId
                        )
                    , viewDetail "Status"
                        (if access.status == 200 then
                            "200 OK"

                         else
                            String.fromInt access.status
                        )
                    , case access.error of
                        Just err ->
                            viewDetail "Error" err

                        Nothing ->
                            viewDetail "Result" "Success"
                    , viewDetail "Time" access.timestamp
                    ]

            Nothing ->
                div [ class "resource-empty" ]
                    [ text "No resource requests yet" ]
        ]


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
            [ span [ class "client-label" ] [ text ("Client " ++ String.fromInt client.id) ]
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
