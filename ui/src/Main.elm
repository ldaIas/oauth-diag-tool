module Main exposing (main)

import Browser
import Html exposing (..)
import Html.Attributes exposing (class)
import Html.Events exposing (onClick)
import ServerForm


-- MODEL


type alias OAuthServer =
    { id : String
    , name : String
    , issuerUrl : String
    , authorizationUrl : String
    , tokenEndpoint : String
    , port_ : Int
    , running : Bool
    }


type Page
    = ServerList
    | CreateForm ServerForm.Model


type alias Model =
    { servers : List OAuthServer
    , page : Page
    }


init : Model
init =
    { servers = []
    , page = ServerList
    }


-- UPDATE


type Msg
    = OpenCreateForm
    | FormMsg ServerForm.Msg


update : Msg -> Model -> Model
update msg model =
    case msg of
        OpenCreateForm ->
            { model | page = CreateForm (ServerForm.init (nextPort model)) }

        FormMsg subMsg ->
            case model.page of
                CreateForm formModel ->
                    let
                        ( newForm, action ) =
                            ServerForm.update subMsg formModel
                    in
                    case action of
                        Just _ ->
                            -- Both Submit and Cancel return to server list for now
                            { model | page = ServerList }

                        Nothing ->
                            { model | page = CreateForm newForm }

                _ ->
                    model


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
        ServerList ->
            div [ class "page-content" ]
                [ viewBody model
                , viewFooter
                ]

        CreateForm formModel ->
            Html.map FormMsg (ServerForm.view formModel)


viewHeader : Html Msg
viewHeader =
    div [ class "header" ]
        [ span [ class "header-title" ] [ text "OAuth Diagnostic" ]
        , span [ class "header-subtitle" ] [ text "authorization server manager" ]
        ]


viewBody : Model -> Html Msg
viewBody model =
    if List.isEmpty model.servers then
        div [ class "empty-state" ]
            [ span [ class "empty-label" ] [ text "No servers configured" ] ]

    else
        div [ class "server-list" ]
            (List.map viewServerCard model.servers)


viewServerCard : OAuthServer -> Html Msg
viewServerCard server =
    div [ class "server-card" ]
        [ div [ class "server-card-left" ]
            [ span [ class "server-name" ] [ text server.name ]
            , span [ class "server-meta" ] [ text ("localhost:" ++ String.fromInt server.port_) ]
            ]
        , div [ class "server-card-right" ]
            [ div [ class ("status-dot" ++ statusClass server.running) ] []
            , span [ class "status-label" ] [ text (statusLabel server.running) ]
            ]
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


-- MAIN


main : Program () Model Msg
main =
    Browser.sandbox
        { init = init
        , update = update
        , view = view
        }
