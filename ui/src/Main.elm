port module Main exposing (main)

import Browser
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



-- MODEL


type Page
    = ServerListPage
    | CreateFormPage ServerForm.Model


type alias Model =
    { serverList : ServerList.Model
    , page : Page
    }


init : () -> ( Model, Cmd Msg )
init _ =
    ( { serverList = ServerList.init
      , page = ServerListPage
      }
    , requestServerConfigs ()
    )



-- UPDATE


type Msg
    = ServerListMsg ServerList.Msg
    | FormMsg ServerForm.Msg


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
                        , page = CreateFormPage (ServerForm.init port_)
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

                ServerList.NoAction ->
                    ( { model | serverList = newServerList }
                    , Cmd.none
                    )

        FormMsg subMsg ->
            case model.page of
                CreateFormPage formModel ->
                    let
                        ( newForm, action ) =
                            ServerForm.update subMsg formModel
                    in
                    case action of
                        Just ServerForm.SubmitForm ->
                            ( { model | page = ServerListPage }
                            , createServerConfig
                                (Encode.object
                                    [ ( "configName", Encode.string newForm.name )
                                    , ( "authServerUrl", Encode.string newForm.authorizationUrl )
                                    , ( "tokenUrl", Encode.string newForm.tokenEndpoint )
                                    ]
                                )
                            )

                        Just ServerForm.CancelForm ->
                            ( { model | page = ServerListPage }
                            , Cmd.none
                            )

                        Nothing ->
                            ( { model | page = CreateFormPage newForm }
                            , Cmd.none
                            )

                _ ->
                    ( model, Cmd.none )




-- VIEW


view : Model -> Html Msg
view model =
    div [ class "shell" ]
        [ viewHeader
        , viewPage model
        ]


viewPage : Model -> Html Msg
viewPage model =
    case model.page of
        ServerListPage ->
            Html.map ServerListMsg (ServerList.view model.serverList)

        CreateFormPage formModel ->
            Html.map FormMsg (ServerForm.view formModel)


viewHeader : Html Msg
viewHeader =
    div [ class "header" ]
        [ span [ class "header-title" ] [ text "OAuth Diagnostic" ]
        , span [ class "header-subtitle" ] [ text "authorization server manager" ]
        ]



-- SUBSCRIPTIONS


subscriptions : Model -> Sub Msg
subscriptions _ =
    receiveServerConfigs
        (\val ->
            ServerListMsg
                (ServerList.GotServerConfigs
                    (Decode.decodeValue (Decode.list ServerList.serverConfigDecoder) val)
                )
        )



-- MAIN


main : Program () Model Msg
main =
    Browser.element
        { init = init
        , update = update
        , subscriptions = subscriptions
        , view = view
        }
