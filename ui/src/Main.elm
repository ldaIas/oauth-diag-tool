module Main exposing (main)

import Browser
import Html exposing (..)
import Html.Attributes exposing (class)
import Html.Events exposing (onClick)


-- MODEL


type alias OAuthServer =
    { id : String
    , name : String
    , port_ : Int
    , running : Bool
    }


type alias Model =
    { servers : List OAuthServer }


init : Model
init =
    { servers = [] }


-- UPDATE


type Msg
    = CreateServer


update : Msg -> Model -> Model
update msg model =
    case msg of
        CreateServer ->
            model


-- VIEW


view : Model -> Html Msg
view model =
    div [ class "shell" ]
        [ viewHeader
        , viewBody model
        , viewFooter
        ]


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
        [ button [ class "btn-create", onClick CreateServer ]
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
