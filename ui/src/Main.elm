port module Main exposing (main)

import Browser
import ClientConfigForm
import ClientConfigList
import Html exposing (..)
import Html.Attributes exposing (class)
import Json.Decode as Decode
import Json.Encode as Encode
import ServerForm
import ServerList



-- PORTS


port createServerConfig : Encode.Value -> Cmd msg


port deleteServerConfig : Encode.Value -> Cmd msg


port addClientToServer : Encode.Value -> Cmd msg


port deleteClient : Encode.Value -> Cmd msg


port requestServerConfigs : () -> Cmd msg


port receiveServerConfigs : (Decode.Value -> msg) -> Sub msg


port createClientConfig : Encode.Value -> Cmd msg


port deleteClientConfig : Encode.Value -> Cmd msg


port requestClientConfigs : () -> Cmd msg


port receiveClientConfigs : (Decode.Value -> msg) -> Sub msg


port startServer : Encode.Value -> Cmd msg


port stopServer : Encode.Value -> Cmd msg



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
    }


init : () -> ( Model, Cmd Msg )
init _ =
    ( { serverList = ServerList.init
      , clientConfigList = ClientConfigList.init
      , leftPage = ServerListPage
      , rightPage = ClientConfigListPage
      }
    , Cmd.batch
        [ requestServerConfigs ()
        , requestClientConfigs ()
        ]
    )



-- UPDATE


type Msg
    = ServerListMsg ServerList.Msg
    | ServerFormMsg ServerForm.Msg
    | ClientConfigListMsg ClientConfigList.Msg
    | ClientConfigFormMsg ClientConfigForm.Msg


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

                ServerList.NoAction ->
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

                ClientConfigList.NoAction ->
                    ( { model | clientConfigList = newConfigList }
                    , Cmd.none
                    )

        ClientConfigFormMsg subMsg ->
            case model.rightPage of
                ClientConfigCreateFormPage formModel ->
                    let
                        ( newForm, action ) =
                            ClientConfigForm.update subMsg formModel
                    in
                    case action of
                        Just ClientConfigForm.SubmitForm ->
                            ( { model | rightPage = ClientConfigListPage }
                            , createClientConfig
                                (Encode.object
                                    [ ( "name", Encode.string newForm.name )
                                    , ( "issuerUrl", Encode.string newForm.issuerUrl )
                                    , ( "authorizationUrl", Encode.string newForm.authorizationUrl )
                                    , ( "tokenUrl", Encode.string newForm.tokenUrl )
                                    , ( "clientId", Encode.string newForm.clientId )
                                    , ( "clientSecret", Encode.string newForm.clientSecret )
                                    , ( "scopes", Encode.string newForm.scopes )
                                    , ( "grantType", Encode.string newForm.grantType )
                                    , ( "extraParams", Encode.string (extraParamsToJson newForm.extraParams) )
                                    ]
                                )
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
            [ div [ class "panel" ]
                [ div [ class "panel-title" ] [ text "OAuth Servers" ]
                , viewLeftPage model
                ]
            , div [ class "panel" ]
                [ div [ class "panel-title" ] [ text "Client Configurations" ]
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
            Html.map ClientConfigListMsg (ClientConfigList.view model.clientConfigList)

        ClientConfigCreateFormPage formModel ->
            Html.map ClientConfigFormMsg (ClientConfigForm.view formModel)


viewHeader : Html Msg
viewHeader =
    div [ class "header" ]
        [ span [ class "header-title" ] [ text "OAuth Diagnostic" ]
        , span [ class "header-subtitle" ] [ text "authorization server manager" ]
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
