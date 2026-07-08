-- Copyright 2026 Sam Sovereign
-- SPDX-License-Identifier: Apache-2.0


port module Main exposing (main)

import Browser
import ClientConfigForm
import ClientConfigList
import Html exposing (..)
import Html.Attributes exposing (class, title)
import Html.Events exposing (onClick)
import Json.Decode as Decode
import Json.Encode as Encode
import Process
import ServerForm
import ServerList
import Task



-- PORTS


port createServerConfig : Encode.Value -> Cmd msg


port deleteServerConfig : Encode.Value -> Cmd msg


port addClientToServer : Encode.Value -> Cmd msg


port deleteClient : Encode.Value -> Cmd msg


port requestServerConfigs : () -> Cmd msg


port receiveServerConfigs : (Decode.Value -> msg) -> Sub msg


port createClientConfig : Encode.Value -> Cmd msg


port updateClientConfig : Encode.Value -> Cmd msg


port deleteClientConfig : Encode.Value -> Cmd msg


port requestClientConfigs : () -> Cmd msg


port receiveClientConfigs : (Decode.Value -> msg) -> Sub msg


port startServer : Encode.Value -> Cmd msg


port stopServer : Encode.Value -> Cmd msg


port updateServerSettings : Encode.Value -> Cmd msg


port serverSettingsSaved : (Decode.Value -> msg) -> Sub msg


port authorizeClient : Encode.Value -> Cmd msg


port cancelAuthorization : Encode.Value -> Cmd msg


port requestCallbackUrl : () -> Cmd msg


port receiveCallbackUrl : (Decode.Value -> msg) -> Sub msg


port receiveAuthResult : (Decode.Value -> msg) -> Sub msg


port refreshToken : Encode.Value -> Cmd msg


port fetchServerMetadata : Encode.Value -> Cmd msg


port receiveServerMetadata : (Decode.Value -> msg) -> Sub msg


port receiveResourceAccess : (Decode.Value -> msg) -> Sub msg


port notifyError : (Decode.Value -> msg) -> Sub msg



-- MODEL


type LeftPage
    = ServerListPage
    | ServerCreateFormPage ServerForm.Model


type RightPage
    = ClientConfigListPage
    | ClientConfigCreateFormPage ClientConfigForm.Model


type alias Model =
    { serverList : ServerList.Model
    , clientConfigList : ClientConfigList.Model
    , leftPage : LeftPage
    , rightPage : RightPage
    , callbackUrl : String
    , leftCollapsed : Bool
    , rightCollapsed : Bool
    , pendingFormSubmit : Maybe ClientConfigForm.Model
    , lastError : Maybe String
    }


init : () -> ( Model, Cmd Msg )
init _ =
    ( { serverList = ServerList.init
      , clientConfigList = ClientConfigList.init
      , leftPage = ServerListPage
      , rightPage = ClientConfigListPage
      , callbackUrl = ""
      , leftCollapsed = False
      , rightCollapsed = False
      , pendingFormSubmit = Nothing
      , lastError = Nothing
      }
    , Cmd.batch
        [ requestServerConfigs ()
        , requestClientConfigs ()
        , requestCallbackUrl ()
        ]
    )



-- UPDATE


type Msg
    = ServerListMsg ServerList.Msg
    | ServerFormMsg ServerForm.Msg
    | ClientConfigListMsg ClientConfigList.Msg
    | ClientConfigFormMsg ClientConfigForm.Msg
    | GotCallbackUrl String
    | GotServerMetadata (Result Decode.Error ClientConfigList.MetadataResult)
    | SettingsSavedAck Int
    | BackendError String
    | DismissError
    | ToggleLeftPanel
    | ToggleRightPanel


update : Msg -> Model -> ( Model, Cmd Msg )
update msg model =
    case msg of
        ServerListMsg subMsg ->
            let
                ( newServerList, action ) =
                    ServerList.update subMsg model.serverList
            in
            applyServerListAction { model | serverList = newServerList } action

        ServerFormMsg subMsg ->
            case model.leftPage of
                ServerCreateFormPage formModel ->
                    let
                        ( newForm, action ) =
                            ServerForm.update subMsg formModel
                    in
                    case action of
                        Just ServerForm.SubmitForm ->
                            ( { model | leftPage = ServerListPage }
                            , createServerConfig
                                (Encode.object
                                    [ ( "configName", Encode.string newForm.name )
                                    , ( "authServerUrl", Encode.string newForm.authorizationUrl )
                                    , ( "tokenUrl", Encode.string newForm.tokenEndpoint )
                                    ]
                                )
                            )

                        Just ServerForm.CancelForm ->
                            ( { model | leftPage = ServerListPage }
                            , Cmd.none
                            )

                        Nothing ->
                            ( { model | leftPage = ServerCreateFormPage newForm }
                            , Cmd.none
                            )

                _ ->
                    ( model, Cmd.none )

        ClientConfigListMsg subMsg ->
            let
                ( newConfigList, action ) =
                    ClientConfigList.update subMsg model.clientConfigList
            in
            applyConfigListAction { model | clientConfigList = newConfigList } action

        GotCallbackUrl url ->
            ( { model | callbackUrl = url }, Cmd.none )

        ToggleLeftPanel ->
            ( { model | leftCollapsed = not model.leftCollapsed }, Cmd.none )

        ToggleRightPanel ->
            ( { model | rightCollapsed = not model.rightCollapsed }, Cmd.none )

        SettingsSavedAck serverId ->
            let
                ( newServerList, _ ) =
                    ServerList.update (ServerList.SettingsSaved serverId) model.serverList
            in
            ( { model | serverList = newServerList }
            , Process.sleep 2000
                |> Task.perform (\_ -> ServerListMsg (ServerList.ClearSettingsSaved serverId))
            )

        BackendError err ->
            ( { model | lastError = Just err }, Cmd.none )

        DismissError ->
            ( { model | lastError = Nothing }, Cmd.none )

        GotServerMetadata (Err decodeError) ->
            ( { model | lastError = Just ("Failed to decode metadata response: " ++ Decode.errorToString decodeError) }
            , Cmd.none
            )

        GotServerMetadata (Ok metadata) ->
            case metadata.configId of
                Nothing ->
                    -- Metadata was fetched as part of a form submission
                    handleFormMetadata metadata.outcome model

                Just _ ->
                    -- Refresh-metadata flow on an existing config card
                    let
                        ( newConfigList, action ) =
                            ClientConfigList.update (ClientConfigList.GotMetadataResult metadata) model.clientConfigList
                    in
                    applyConfigListAction { model | clientConfigList = newConfigList } action

        ClientConfigFormMsg subMsg ->
            case model.rightPage of
                ClientConfigCreateFormPage formModel ->
                    let
                        ( newForm, action ) =
                            ClientConfigForm.update subMsg formModel
                    in
                    case action of
                        Just ClientConfigForm.SubmitForm ->
                            if newForm.useServerMetadata then
                                -- Fetch metadata first; the config is saved when it arrives
                                ( { model
                                    | pendingFormSubmit = Just newForm
                                    , rightPage = ClientConfigListPage
                                  }
                                , fetchServerMetadata
                                    (Encode.object
                                        [ ( "issuerUrl", Encode.string newForm.issuerUrl )
                                        , ( "configId", Encode.null )
                                        ]
                                    )
                                )

                            else
                                ( { model | rightPage = ClientConfigListPage }
                                , saveConfigCmd newForm.editingId
                                    (ClientConfigList.encodeConfigFields (ClientConfigForm.toConfigFields newForm))
                                )

                        Just ClientConfigForm.CancelForm ->
                            ( { model | rightPage = ClientConfigListPage }
                            , Cmd.none
                            )

                        Nothing ->
                            ( { model | rightPage = ClientConfigCreateFormPage newForm }
                            , Cmd.none
                            )

                _ ->
                    ( model, Cmd.none )


applyServerListAction : Model -> ServerList.Action -> ( Model, Cmd Msg )
applyServerListAction model action =
    case action of
        ServerList.RequestCreateForm port_ ->
            ( { model | leftPage = ServerCreateFormPage (ServerForm.init port_) }
            , Cmd.none
            )

        ServerList.RequestDeleteServer id ->
            ( model, deleteServerConfig (Encode.object [ ( "id", Encode.int id ) ]) )

        ServerList.RequestAddClient serverId ->
            ( model, addClientToServer (Encode.object [ ( "authServerId", Encode.int serverId ) ]) )

        ServerList.RequestDeleteClient clientId ->
            ( model, deleteClient (Encode.object [ ( "id", Encode.int clientId ) ]) )

        ServerList.RequestImportClient data ->
            ( { model | rightPage = ClientConfigCreateFormPage (ClientConfigForm.initFromImport data) }
            , Cmd.none
            )

        ServerList.RequestStartServer id ->
            ( model, startServer (Encode.object [ ( "id", Encode.int id ) ]) )

        ServerList.RequestStopServer id ->
            ( model, stopServer (Encode.object [ ( "id", Encode.int id ) ]) )

        ServerList.RequestSaveSettings settings ->
            ( model
            , updateServerSettings
                (Encode.object
                    [ ( "id", Encode.int settings.id )
                    , ( "redirectUrlOverride", Encode.string settings.redirectUrlOverride )
                    , ( "accessTokenExpiry", Encode.int settings.accessTokenExpiry )
                    , ( "refreshTokenExpiry", Encode.int settings.refreshTokenExpiry )
                    ]
                )
            )

        ServerList.NoAction ->
            ( model, Cmd.none )


applyConfigListAction : Model -> ClientConfigList.Action -> ( Model, Cmd Msg )
applyConfigListAction model action =
    case action of
        ClientConfigList.RequestCreateForm ->
            ( { model | rightPage = ClientConfigCreateFormPage ClientConfigForm.init }
            , Cmd.none
            )

        ClientConfigList.RequestDeleteConfig id ->
            ( model, deleteClientConfig (Encode.object [ ( "id", Encode.int id ) ]) )

        ClientConfigList.RequestEditConfig config ->
            ( { model | rightPage = ClientConfigCreateFormPage (ClientConfigForm.initFromEdit config) }
            , Cmd.none
            )

        ClientConfigList.RequestAuthorizeConfig id ->
            ( model, authorizeClient (Encode.object [ ( "id", Encode.int id ) ]) )

        ClientConfigList.RequestCancelAuthorize id ->
            ( model, cancelAuthorization (Encode.object [ ( "id", Encode.int id ) ]) )

        ClientConfigList.RequestUpdateConfig config ->
            ( model, saveConfigCmd (Just config.id) (ClientConfigList.encodeConfigFields config) )

        ClientConfigList.RequestRefreshToken id ->
            ( model, refreshToken (Encode.object [ ( "id", Encode.int id ) ]) )

        ClientConfigList.RequestFetchMetadata configId issuerUrl ->
            ( model
            , fetchServerMetadata
                (Encode.object
                    [ ( "issuerUrl", Encode.string issuerUrl )
                    , ( "configId", Encode.int configId )
                    ]
                )
            )

        ClientConfigList.NoAction ->
            ( model, Cmd.none )


{-| Save a config: create when there is no id, update when there is one.
Accepts either pre-encoded fields from `ClientConfigList.encodeConfigFields`.
-}
saveConfigCmd : Maybe Int -> List ( String, Encode.Value ) -> Cmd Msg
saveConfigCmd editingId fields =
    case editingId of
        Just id ->
            updateClientConfig
                (Encode.object
                    [ ( "id", Encode.int id )
                    , ( "config", Encode.object fields )
                    ]
                )

        Nothing ->
            createClientConfig (Encode.object [ ( "config", Encode.object fields ) ])


{-| Complete a form submission once its metadata fetch resolves.
-}
handleFormMetadata : Result String ClientConfigList.MetadataPayload -> Model -> ( Model, Cmd Msg )
handleFormMetadata outcome model =
    case model.pendingFormSubmit of
        Nothing ->
            ( model, Cmd.none )

        Just pendingForm ->
            case outcome of
                Err err ->
                    -- Re-show the form so nothing typed is lost
                    ( { model
                        | pendingFormSubmit = Nothing
                        , rightPage = ClientConfigCreateFormPage pendingForm
                        , lastError = Just ("Metadata fetch failed: " ++ err)
                      }
                    , Cmd.none
                    )

                Ok payload ->
                    let
                        base =
                            ClientConfigForm.toConfigFields pendingForm

                        fields =
                            ClientConfigList.encodeConfigFields
                                { base
                                    | authorizationUrl = payload.authorizationEndpoint
                                    , tokenUrl = payload.tokenEndpoint
                                    , scopesSupported = payload.scopesSupported
                                }
                    in
                    ( { model | pendingFormSubmit = Nothing, rightPage = ClientConfigListPage }
                    , saveConfigCmd pendingForm.editingId fields
                    )



-- VIEW


view : Model -> Html Msg
view model =
    div [ class "shell" ]
        [ viewHeader
        , viewErrorBanner model.lastError
        , div [ class "split-layout" ]
            [ if model.leftCollapsed then
                div [ class "panel-collapsed panel-collapsed-left", onClick ToggleLeftPanel, title "Expand OAuth Servers" ]
                    [ span [ class "panel-collapsed-label" ] [ text "OAuth Server Configurations" ]
                    , span [ class "panel-collapsed-arrow" ] [ text "›" ]
                    ]

              else
                div
                    [ class
                        (if model.rightCollapsed then
                            "panel panel-expanded"

                         else
                            "panel"
                        )
                    ]
                    [ div [ class "panel-title" ]
                        [ text "OAuth Server Configurations"
                        , span [ class "panel-collapse-btn", onClick ToggleLeftPanel, title "Collapse" ] [ text "‹" ]
                        ]
                    , viewLeftPage model
                    ]
            , if model.rightCollapsed then
                div [ class "panel-collapsed panel-collapsed-right", onClick ToggleRightPanel, title "Expand Client Configurations" ]
                    [ span [ class "panel-collapsed-arrow" ] [ text "‹" ]
                    , span [ class "panel-collapsed-label" ] [ text "OAuth Client Configurations" ]
                    ]

              else
                div
                    [ class
                        (if model.leftCollapsed then
                            "panel panel-expanded"

                         else
                            "panel"
                        )
                    ]
                    [ div [ class "panel-title" ]
                        [ text "OAuth Client Configurations"
                        , span [ class "panel-collapse-btn", onClick ToggleRightPanel, title "Collapse" ] [ text "›" ]
                        ]
                    , viewRightPage model
                    ]
            ]
        ]


viewErrorBanner : Maybe String -> Html Msg
viewErrorBanner lastError =
    case lastError of
        Nothing ->
            text ""

        Just err ->
            div [ class "error-banner" ]
                [ span [ class "error-banner-text" ] [ text err ]
                , span [ class "error-banner-dismiss", onClick DismissError, title "Dismiss" ] [ text "✕" ]
                ]


viewLeftPage : Model -> Html Msg
viewLeftPage model =
    case model.leftPage of
        ServerListPage ->
            Html.map ServerListMsg (ServerList.view model.serverList)

        ServerCreateFormPage formModel ->
            Html.map ServerFormMsg (ServerForm.view formModel)


viewRightPage : Model -> Html Msg
viewRightPage model =
    case model.rightPage of
        ClientConfigListPage ->
            Html.map ClientConfigListMsg (ClientConfigList.view model.callbackUrl model.clientConfigList)

        ClientConfigCreateFormPage formModel ->
            Html.map ClientConfigFormMsg (ClientConfigForm.view model.callbackUrl formModel)


viewHeader : Html Msg
viewHeader =
    div [ class "header" ]
        [ span [ class "header-title" ] [ text "OAuth Diagnostic" ]
        , span [ class "header-subtitle" ] [ text "authorization server and client manager" ]
        ]



-- SUBSCRIPTIONS


{-| Decode an incoming port value, reporting failures via the error banner
instead of dropping them silently.
-}
decodeOr : Decode.Decoder a -> (a -> Msg) -> String -> Decode.Value -> Msg
decodeOr decoder toMsg context val =
    case Decode.decodeValue decoder val of
        Ok decoded ->
            toMsg decoded

        Err err ->
            BackendError (context ++ ": " ++ Decode.errorToString err)


subscriptions : Model -> Sub Msg
subscriptions _ =
    Sub.batch
        [ receiveServerConfigs
            (decodeOr (Decode.list ServerList.serverConfigDecoder)
                (ServerListMsg << ServerList.GotServerConfigs)
                "Failed to decode server configs"
            )
        , receiveClientConfigs
            (decodeOr (Decode.list ClientConfigList.clientConfigDecoder)
                (ClientConfigListMsg << ClientConfigList.GotClientConfigs)
                "Failed to decode client configs"
            )
        , receiveAuthResult
            (decodeOr ClientConfigList.authResultDecoder
                (ClientConfigListMsg << ClientConfigList.GotAuthResult)
                "Failed to decode authorization result"
            )
        , receiveServerMetadata
            (\val -> GotServerMetadata (Decode.decodeValue ClientConfigList.metadataResultDecoder val))
        , receiveCallbackUrl
            (\val ->
                case Decode.decodeValue Decode.string val of
                    Ok url ->
                        GotCallbackUrl url

                    Err _ ->
                        GotCallbackUrl ""
            )
        , receiveResourceAccess
            (decodeOr ServerList.resourceAccessDecoder
                (ServerListMsg << ServerList.GotResourceAccess)
                "Failed to decode resource access event"
            )
        , serverSettingsSaved
            (decodeOr Decode.int SettingsSavedAck "Failed to decode settings-saved ack")
        , notifyError
            (\val ->
                BackendError
                    (Decode.decodeValue Decode.string val
                        |> Result.withDefault "Unknown backend error"
                    )
            )
        ]



-- MAIN


main : Program () Model Msg
main =
    Browser.element
        { init = init
        , update = update
        , subscriptions = subscriptions
        , view = view
        }
