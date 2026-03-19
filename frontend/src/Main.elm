
import Browser
import Http

main : Program () Model Msg
main =
    Browser.element
        { init = \_ -> ( init, Cmd.none )
        , update = update
        , view = view
        , subscriptions = \_ -> Sub.none
        }


type alias Model =
    { somethin : String
    }


init : Model
init =
    { somethin = ""
    }


update : Msg -> Model -> ( Model, Cmd Msg )
update msg model =
    (model, Cmd.none)

view : Model -> Html Msg
view model = 
    div [] []