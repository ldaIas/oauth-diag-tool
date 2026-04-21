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
import Task
import ServerList



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


port authorizeClient : Encode.Value -> Cmd msg


port cancelAuthorization : Encode.Value -> Cmd msg


port requestCallbackUrl : () -> Cmd msg


port receiveCallbackUrl : (Decode.Value -> msg) -> Sub msg


port receiveAuthResult : (Decode.Value -> msg) -> Sub msg


port refreshToken : Encode.Value -> Cmd msg


port fetchServerMetadata : Encode.Value -> Cmd msg


port receiveServerMetadata : (Decode.Value -> msg) -> Sub msg


port receiveResourceAccess : (Decode.Value -> msg) -> Sub msg



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
    , pendingSaveSettings : Maybe String
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
      , pendingSaveSettings = Nothing
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
            case action of
                ServerList.RequestCreateForm port_ ->
                    ( { model
                        | serverList = newServerList
                        , leftPage = ServerCreateFormPage (ServerForm.init port_)
                      }
                    , Cmd.none
                    )

                ServerList.RequestDeleteServer id ->
                    ( { model | serverList = newServerList }
                    , deleteServerConfig (Encode.object [ ( "id", Encode.string id ) ])
                    )

                ServerList.RequestAddClient serverId ->
                    ( { model | serverList = newServerList }
                    , addClientToServer (Encode.object [ ( "authServerId", Encode.string serverId ) ])
                    )

                ServerList.RequestDeleteClient clientId ->
                    ( { model | serverList = newServerList }
                    , deleteClient (Encode.object [ ( "id", Encode.string clientId ) ])
                    )

                ServerList.RequestImportClient data ->
                    ( { model
                        | serverList = newServerList
                        , rightPage = ClientConfigCreateFormPage (ClientConfigForm.initFromImport data)
                      }
                    , Cmd.none
                    )

                ServerList.RequestStartServer id ->
                    ( { model | serverList = newServerList }
                    , startServer (Encode.object [ ( "id", Encode.string id ) ])
                    )

                ServerList.RequestStopServer id ->
                    ( { model | serverList = newServerList }
                    , stopServer (Encode.object [ ( "id", Encode.string id ) ])
                    )

                ServerList.RequestSaveSettings settings ->
                    ( { model | serverList = newServerList, pendingSaveSettings = Just settings.id }
                    , updateServerSettings
                        (Encode.object
                            [ ( "id", Encode.string settings.id )
                            , ( "redirectUrlOverride", Encode.string settings.redirectUrlOverride )
                            , ( "accessTokenExpiry", Encode.int settings.accessTokenExpiry )
                            , ( "refreshTokenExpiry", Encode.int settings.refreshTokenExpiry )
                            ]
                        )
                    )

                ServerList.NoAction ->
                    case model.pendingSaveSettings of
                        Just savedId ->
                            let
                                ( confirmedList, _ ) =
                                    ServerList.update (ServerList.SettingsSaved savedId) newServerList
                            in
                            ( { model
                                | serverList = confirmedList
                                , pendingSaveSettings = Nothing
                              }
                            , Process.sleep 2000
                                |> Task.perform (\_ -> ServerListMsg (ServerList.ClearSettingsSaved savedId))
                            )

                        Nothing ->
                            ( { model | serverList = newServerList }
                            , Cmd.none
                            )

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
            case action of
                ClientConfigList.RequestCreateForm ->
                    ( { model
                        | clientConfigList = newConfigList
                        , rightPage = ClientConfigCreateFormPage ClientConfigForm.init
                      }
                    , Cmd.none
                    )

                ClientConfigList.RequestDeleteConfig id ->
                    ( { model | clientConfigList = newConfigList }
                    , deleteClientConfig (Encode.object [ ( "id", Encode.string id ) ])
                    )

                ClientConfigList.RequestEditConfig config ->
                    ( { model
                        | clientConfigList = newConfigList
                        , rightPage =
                            ClientConfigCreateFormPage
                                (ClientConfigForm.initFromEdit
                                    { id = config.id
                                    , name = config.name
                                    , issuerUrl = config.issuerUrl
                                    , authorizationUrl = config.authorizationUrl
                                    , tokenUrl = config.tokenUrl
                                    , clientId = config.clientId
                                    , clientSecret = config.clientSecret
                                    , scopes = config.scopes
                                    , grantType = config.grantType
                                    , extraParams = config.extraParams
                                    , disabledParams = config.disabledParams
                                    , disabledTokenParams = config.disabledTokenParams
                                    }
                                )
                      }
                    , Cmd.none
                    )

                ClientConfigList.RequestAuthorizeConfig id ->
                    ( { model | clientConfigList = newConfigList }
                    , authorizeClient (Encode.object [ ( "id", Encode.string id ) ])
                    )

                ClientConfigList.RequestCancelAuthorize id ->
                    ( { model | clientConfigList = newConfigList }
                    , cancelAuthorization (Encode.object [ ( "id", Encode.string id ) ])
                    )

                ClientConfigList.RequestUpdateConfig config ->
                    ( { model | clientConfigList = newConfigList }
                    , updateClientConfig
                        (Encode.object
                            [ ( "id", Encode.string config.id )
                            , ( "name", Encode.string config.name )
                            , ( "issuerUrl", Encode.string config.issuerUrl )
                            , ( "authorizationUrl", Encode.string config.authorizationUrl )
                            , ( "tokenUrl", Encode.string config.tokenUrl )
                            , ( "clientId", Encode.string config.clientId )
                            , ( "clientSecret", Encode.string config.clientSecret )
                            , ( "scopes", Encode.string config.scopes )
                            , ( "grantType", Encode.string config.grantType )
                            , ( "extraParams", Encode.string config.extraParams )
                            , ( "disabledParams", Encode.string config.disabledParams )
                            , ( "disabledTokenParams", Encode.string config.disabledTokenParams )
                            , ( "scopesSupported", Encode.string config.scopesSupported )
                            ]
                        )
                    )

                ClientConfigList.RequestRefreshToken id ->
                    ( { model | clientConfigList = newConfigList }
                    , refreshToken (Encode.object [ ( "id", Encode.string id ) ])
                    )

                ClientConfigList.RequestFetchMetadata configId issuerUrl ->
                    ( { model | clientConfigList = newConfigList }
                    , fetchServerMetadata
                        (Encode.object
                            [ ( "issuerUrl", Encode.string issuerUrl )
                            , ( "configId", Encode.string configId )
                            ]
                        )
                    )

                ClientConfigList.NoAction ->
                    ( { model | clientConfigList = newConfigList }
                    , Cmd.none
                    )

        GotCallbackUrl url ->
            ( { model | callbackUrl = url }, Cmd.none )

        ToggleLeftPanel ->
            ( { model | leftCollapsed = not model.leftCollapsed }, Cmd.none )

        ToggleRightPanel ->
            ( { model | rightCollapsed = not model.rightCollapsed }, Cmd.none )

        GotServerMetadata result ->
            case result of
                Ok metadata ->
                    if String.isEmpty metadata.configId then
                        -- Form submission flow
                        if String.isEmpty metadata.authorizationEndpoint && String.isEmpty metadata.tokenEndpoint then
                            -- Metadata fetch failed: re-show form
                            case model.pendingFormSubmit of
                                Just pendingForm ->
                                    ( { model
                                        | pendingFormSubmit = Nothing
                                        , rightPage = ClientConfigCreateFormPage pendingForm
                                      }
                                    , Cmd.none
                                    )

                                Nothing ->
                                    ( model, Cmd.none )

                        else
                            -- Metadata fetched, now save the config
                            case model.pendingFormSubmit of
                                Just pendingForm ->
                                    let
                                        formWithMetadata =
                                            { pendingForm
                                                | authorizationUrl = metadata.authorizationEndpoint
                                                , tokenUrl = metadata.tokenEndpoint
                                            }

                                        fields =
                                            [ ( "name", Encode.string formWithMetadata.name )
                                            , ( "issuerUrl", Encode.string formWithMetadata.issuerUrl )
                                            , ( "authorizationUrl", Encode.string formWithMetadata.authorizationUrl )
                                            , ( "tokenUrl", Encode.string formWithMetadata.tokenUrl )
                                            , ( "clientId", Encode.string formWithMetadata.clientId )
                                            , ( "clientSecret", Encode.string formWithMetadata.clientSecret )
                                            , ( "scopes", Encode.string formWithMetadata.scopes )
                                            , ( "grantType", Encode.string formWithMetadata.grantType )
                                            , ( "extraParams", Encode.string (extraParamsToJson formWithMetadata.extraParams) )
                                            , ( "disabledParams", Encode.string formWithMetadata.disabledParams )
                                            , ( "disabledTokenParams", Encode.string formWithMetadata.disabledTokenParams )
                                            , ( "scopesSupported", Encode.string metadata.scopesSupported )
                                            ]

                                        cmd =
                                            case formWithMetadata.editingId of
                                                Just id ->
                                                    updateClientConfig
                                                        (Encode.object (( "id", Encode.string id ) :: fields))

                                                Nothing ->
                                                    createClientConfig
                                                        (Encode.object fields)
                                    in
                                    ( { model | pendingFormSubmit = Nothing, rightPage = ClientConfigListPage }
                                    , cmd
                                    )

                                Nothing ->
                                    ( model, Cmd.none )

                    else
                        -- List refresh flow: forward to ClientConfigList
                        let
                            ( newConfigList, action ) =
                                ClientConfigList.update (ClientConfigList.GotMetadataResult (Ok metadata)) model.clientConfigList
                        in
                        case action of
                            ClientConfigList.RequestUpdateConfig config ->
                                ( { model | clientConfigList = newConfigList }
                                , updateClientConfig
                                    (Encode.object
                                        [ ( "id", Encode.string config.id )
                                        , ( "name", Encode.string config.name )
                                        , ( "issuerUrl", Encode.string config.issuerUrl )
                                        , ( "authorizationUrl", Encode.string config.authorizationUrl )
                                        , ( "tokenUrl", Encode.string config.tokenUrl )
                                        , ( "clientId", Encode.string config.clientId )
                                        , ( "clientSecret", Encode.string config.clientSecret )
                                        , ( "scopes", Encode.string config.scopes )
                                        , ( "grantType", Encode.string config.grantType )
                                        , ( "extraParams", Encode.string config.extraParams )
                                        , ( "disabledParams", Encode.string config.disabledParams )
                                        , ( "disabledTokenParams", Encode.string config.disabledTokenParams )
                                        , ( "scopesSupported", Encode.string config.scopesSupported )
                                        ]
                                    )
                                )

                            _ ->
                                ( { model | clientConfigList = newConfigList }, Cmd.none )

                Err _ ->
                    case model.pendingFormSubmit of
                        Just pendingForm ->
                            -- Metadata fetch failed during form submission: re-show form
                            ( { model
                                | pendingFormSubmit = Nothing
                                , rightPage = ClientConfigCreateFormPage pendingForm
                              }
                            , Cmd.none
                            )

                        Nothing ->
                            ( model, Cmd.none )

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
                                -- Fetch metadata first, then save
                                ( { model
                                    | pendingFormSubmit = Just newForm
                                    , rightPage = ClientConfigListPage
                                  }
                                , fetchServerMetadata
                                    (Encode.object
                                        [ ( "issuerUrl", Encode.string newForm.issuerUrl )
                                        , ( "configId", Encode.string "" )
                                        ]
                                    )
                                )

                            else
                                let
                                    fields =
                                        [ ( "name", Encode.string newForm.name )
                                        , ( "issuerUrl", Encode.string newForm.issuerUrl )
                                        , ( "authorizationUrl", Encode.string newForm.authorizationUrl )
                                        , ( "tokenUrl", Encode.string newForm.tokenUrl )
                                        , ( "clientId", Encode.string newForm.clientId )
                                        , ( "clientSecret", Encode.string newForm.clientSecret )
                                        , ( "scopes", Encode.string newForm.scopes )
                                        , ( "grantType", Encode.string newForm.grantType )
                                        , ( "extraParams", Encode.string (extraParamsToJson newForm.extraParams) )
                                        , ( "disabledParams", Encode.string newForm.disabledParams )
                                        , ( "disabledTokenParams", Encode.string newForm.disabledTokenParams )
                                        , ( "scopesSupported", Encode.string "" )
                                        ]

                                    cmd =
                                        case newForm.editingId of
                                            Just id ->
                                                updateClientConfig
                                                    (Encode.object (( "id", Encode.string id ) :: fields))

                                            Nothing ->
                                                createClientConfig
                                                    (Encode.object fields)
                                in
                                ( { model | rightPage = ClientConfigListPage }
                                , cmd
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


extraParamsToJson : List { key : String, value : String } -> String
extraParamsToJson params =
    let
        nonEmpty =
            List.filter (\p -> not (String.isEmpty p.key)) params

        pairs =
            List.map (\p -> "\"" ++ p.key ++ "\":\"" ++ p.value ++ "\"") nonEmpty
    in
    if List.isEmpty pairs then
        ""

    else
        "{" ++ String.join "," pairs ++ "}"



-- VIEW


view : Model -> Html Msg
view model =
    div [ class "shell" ]
        [ viewHeader
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


subscriptions : Model -> Sub Msg
subscriptions _ =
    Sub.batch
        [ receiveServerConfigs
            (\val ->
                ServerListMsg
                    (ServerList.GotServerConfigs
                        (Decode.decodeValue (Decode.list ServerList.serverConfigDecoder) val)
                    )
            )
        , receiveClientConfigs
            (\val ->
                ClientConfigListMsg
                    (ClientConfigList.GotClientConfigs
                        (Decode.decodeValue (Decode.list ClientConfigList.clientConfigDecoder) val)
                    )
            )
        , receiveAuthResult
            (\val ->
                ClientConfigListMsg
                    (ClientConfigList.GotAuthResult
                        (Decode.decodeValue ClientConfigList.authResultDecoder val)
                    )
            )
        , receiveServerMetadata
            (\val ->
                GotServerMetadata
                    (Decode.decodeValue ClientConfigList.metadataResultDecoder val)
            )
        , receiveCallbackUrl
            (\val ->
                case Decode.decodeValue Decode.string val of
                    Ok url ->
                        GotCallbackUrl url

                    Err _ ->
                        GotCallbackUrl ""
            )
        , receiveResourceAccess
            (\val ->
                ServerListMsg
                    (ServerList.GotResourceAccess
                        (Decode.decodeValue ServerList.resourceAccessDecoder val)
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
